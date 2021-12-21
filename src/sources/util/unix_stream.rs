use std::{fs::remove_file, path::PathBuf, time::Duration};

use bytes::Bytes;
use futures::{FutureExt, SinkExt, StreamExt};
use tokio::{
    io::AsyncWriteExt,
    net::{UnixListener, UnixStream},
    time::sleep,
};
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::codec::FramedRead;
use tracing::field;
use tracing_futures::Instrument;

use crate::{
    async_read::VecAsyncReadExt,
    codecs,
    event::Event,
    internal_events::{ConnectionOpen, OpenGauge, UnixSocketError, UnixSocketFileDeleteError},
    shutdown::ShutdownSignal,
    sources::{util::codecs::StreamDecodingError, Source},
    Pipeline,
};

/// Returns a `Source` object corresponding to a Unix domain stream socket.
/// Passing in different functions for `decoder` and `handle_events` can allow
/// for different source-specific logic (such as decoding syslog messages in the
/// syslog source).
pub fn build_unix_stream_source(
    listen_path: PathBuf,
    decoder: codecs::Decoder,
    handle_events: impl Fn(&mut [Event], Option<Bytes>, usize) + Clone + Send + Sync + 'static,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    let out = out.sink_map_err(|error| error!(message = "Error sending line.", %error));

    Box::pin(async move {
        let listener = UnixListener::bind(&listen_path).expect("Failed to bind to listener socket");
        info!(message = "Listening.", path = ?listen_path, r#type = "unix");

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

            let stream = socket.allow_read_until(shutdown.clone().map(|_| ()));
            let mut stream = FramedRead::new(stream, decoder.clone());

            let connection_open = connection_open.clone();
            let mut out = out.clone();
            tokio::spawn(
                async move {
                    let _open_token =
                        connection_open.open(|count| emit!(&ConnectionOpen { count }));

                    loop {
                        match stream.next().await {
                            Some(Ok((mut events, byte_size))) => {
                                handle_events(&mut events, received_from.clone(), byte_size);

                                for event in events {
                                    let _ = out.send(event).await;
                                }
                            }
                            Some(Err(error)) => {
                                emit!(&UnixSocketError {
                                    error: &error,
                                    path: &listen_path
                                });

                                if !error.can_continue() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }

                    info!("Finished sending.");

                    let socket: &mut UnixStream = stream.get_mut().get_mut();
                    if let Err(error) = socket.shutdown().await {
                        error!(message = "Failed shutting down socket.", %error);
                    }
                }
                .instrument(span),
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
            emit!(&UnixSocketFileDeleteError {
                path: &listen_path,
                error
            });
        }

        Ok(())
    })
}
