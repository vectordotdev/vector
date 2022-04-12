use std::{fs::remove_file, path::PathBuf, time::Duration};

use bytes::Bytes;
use codecs::StreamDecodingError;
use futures::{FutureExt, StreamExt};
use tokio::{
    io::AsyncWriteExt,
    net::{UnixListener, UnixStream},
    time::sleep,
};
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::codec::FramedRead;
use tracing::field;
use tracing_futures::Instrument;
use vector_core::ByteSizeOf;

use super::AfterReadExt;
use crate::{
    async_read::VecAsyncReadExt,
    codecs::Decoder,
    event::Event,
    internal_events::{
        BytesReceived, ConnectionOpen, OpenGauge, SocketEventsReceived, SocketMode,
        StreamClosedError, UnixSocketError, UnixSocketFileDeleteError,
    },
    shutdown::ShutdownSignal,
    sources::util::change_socket_permissions,
    sources::Source,
    SourceSender,
};

/// Returns a `Source` object corresponding to a Unix domain stream socket.
/// Passing in different functions for `decoder` and `handle_events` can allow
/// for different source-specific logic (such as decoding syslog messages in the
/// syslog source).
pub fn build_unix_stream_source(
    listen_path: PathBuf,
    socket_file_mode: Option<u32>,
    decoder: Decoder,
    handle_events: impl Fn(&mut [Event], Option<Bytes>) + Clone + Send + Sync + 'static,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    let listener = UnixListener::bind(&listen_path).expect("Failed to bind to listener socket");
    info!(message = "Listening.", path = ?listen_path, r#type = "unix");

    change_socket_permissions(&listen_path, socket_file_mode)?;

    Ok(Box::pin(async move {
        let connection_open = OpenGauge::new();
        let stream = UnixListenerStream::new(listener).take_until(shutdown.clone());
        tokio::pin!(stream);
        while let Some(socket) = stream.next().await {
            let socket = match socket {
                Err(error) => {
                    error!(message = "Failed to accept socket.", %error);
                    continue;
                }
                Ok(socket) => socket,
            };

            let listen_path = listen_path.clone();

            let span = info_span!("connection");
            let path = if let Ok(addr) = socket.peer_addr() {
                if let Some(path) = addr.as_pathname().map(|e| e.to_owned()) {
                    span.record("peer_path", &field::debug(&path));
                    Some(path)
                } else {
                    None
                }
            } else {
                None
            };

            let handle_events = handle_events.clone();
            let received_from: Option<Bytes> =
                path.map(|p| p.to_string_lossy().into_owned().into());

            let stream = socket
                .after_read(|byte_size| {
                    emit!(BytesReceived {
                        protocol: "unix",
                        byte_size,
                    });
                })
                .allow_read_until(shutdown.clone().map(|_| ()));
            let mut stream = FramedRead::new(stream, decoder.clone());

            let connection_open = connection_open.clone();
            let mut out = out.clone();
            tokio::spawn(
                async move {
                    let _open_token = connection_open.open(|count| emit!(ConnectionOpen { count }));

                    while let Some(result) = stream.next().await {
                        match result {
                            Ok((mut events, _byte_size)) => {
                                emit!(SocketEventsReceived {
                                    mode: SocketMode::Unix,
                                    byte_size: events.size_of(),
                                    count: events.len(),
                                });

                                handle_events(&mut events, received_from.clone());

                                let count = events.len();
                                if let Err(error) = out.send_batch(events).await {
                                    emit!(StreamClosedError { error, count });
                                }
                            }
                            Err(error) => {
                                emit!(UnixSocketError {
                                    error: &error,
                                    path: &listen_path
                                });

                                if !error.can_continue() {
                                    break;
                                }
                            }
                        }
                    }

                    info!("Finished sending.");

                    let socket: &mut UnixStream = stream.get_mut().get_mut().get_mut_ref();
                    if let Err(error) = socket.shutdown().await {
                        error!(message = "Failed shutting down socket.", %error);
                    }
                }
                .instrument(span.or_current()),
            );
        }

        // Cleanup
        drop(stream);

        // Wait for open connections to finish
        while connection_open.any_open() {
            sleep(Duration::from_millis(10)).await;
        }

        // Delete socket file
        if let Err(error) = remove_file(&listen_path) {
            emit!(UnixSocketFileDeleteError {
                path: &listen_path,
                error
            });
        }

        Ok(())
    }))
}
