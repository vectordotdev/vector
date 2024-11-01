use std::{fs::remove_file, path::PathBuf, time::Duration};

use bytes::Bytes;
use futures::{FutureExt, StreamExt};
use tokio::{
    io::AsyncWriteExt,
    net::{UnixListener, UnixStream},
    time::sleep,
};
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::codec::FramedRead;
use tracing::{field, Instrument};
use vector_lib::codecs::StreamDecodingError;
use vector_lib::internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol};
use vector_lib::EstimatedJsonEncodedSizeOf;

use super::AfterReadExt;
use crate::{
    async_read::VecAsyncReadExt,
    codecs::Decoder,
    event::Event,
    internal_events::{
        ConnectionOpen, OpenGauge, SocketEventsReceived, SocketMode, StreamClosedError,
        UnixSocketError, UnixSocketFileDeleteError,
    },
    shutdown::ShutdownSignal,
    sources::util::change_socket_permissions,
    sources::util::unix::UNNAMED_SOCKET_HOST,
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
    Ok(Box::pin(async move {
        let listener = UnixListener::bind(&listen_path).unwrap_or_else(|e| {
            panic!(
                "Failed to bind to listener socket at path: {}. Err: {}",
                listen_path.to_string_lossy(),
                e
            )
        });
        info!(message = "Listening.", path = ?listen_path, r#type = "unix");

        change_socket_permissions(&listen_path, socket_file_mode)
            .expect("Failed to set socket permissions");

        let bytes_received = register!(BytesReceived::from(Protocol::UNIX));

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

            let received_from: Bytes = socket
                .peer_addr()
                .ok()
                .and_then(|addr| {
                    addr.as_pathname().map(|e| e.to_owned()).map({
                        |path| {
                            span.record("peer_path", field::debug(&path));
                            path.to_string_lossy().into_owned().into()
                        }
                    })
                })
                // In most cases, we'll be connecting to this socket from
                // an unnamed socket (a socket not bound to a
                // file). Instead of a filename, we'll surface a specific
                // host value.
                .unwrap_or_else(|| UNNAMED_SOCKET_HOST.into());

            let handle_events = handle_events.clone();

            let bytes_received = bytes_received.clone();
            let stream = socket
                .after_read(move |byte_size| {
                    bytes_received.emit(ByteSize(byte_size));
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
                                    byte_size: events.estimated_json_encoded_size_of(),
                                    count: events.len(),
                                });

                                handle_events(&mut events, Some(received_from.clone()));

                                let count = events.len();
                                if (out.send_batch(events).await).is_err() {
                                    emit!(StreamClosedError { count });
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
