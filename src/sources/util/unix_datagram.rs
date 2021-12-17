use std::{fs::remove_file, path::PathBuf};

use bytes::{Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use tokio::net::UnixDatagram;
use tokio_util::codec::FramedRead;
use tracing::field;

use crate::{
    codecs,
    event::Event,
    internal_events::{SocketMode, SocketReceiveError, UnixSocketFileDeleteError},
    shutdown::ShutdownSignal,
    sources::{util::codecs::StreamDecodingError, Source},
    Pipeline,
};

/// Returns a `Source` object corresponding to a Unix domain datagram socket.
/// Passing in different functions for `decoder` and `handle_events` can allow
/// for different source-specific logic (such as decoding syslog messages in the
/// syslog source).
pub fn build_unix_datagram_source(
    listen_path: PathBuf,
    max_length: usize,
    decoder: codecs::Decoder,
    handle_events: impl Fn(&mut [Event], Option<Bytes>, usize) + Clone + Send + Sync + 'static,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    Box::pin(async move {
        let socket = UnixDatagram::bind(&listen_path).expect("Failed to bind to datagram socket");
        info!(message = "Listening.", path = ?listen_path, r#type = "unix_datagram");

        let result = listen(socket, max_length, decoder, shutdown, handle_events, out).await;

        // Delete socket file.
        if let Err(error) = remove_file(&listen_path) {
            emit!(&UnixSocketFileDeleteError {
                path: &listen_path,
                error
            });
        }

        result
    })
}

async fn listen(
    socket: UnixDatagram,
    max_length: usize,
    decoder: codecs::Decoder,
    mut shutdown: ShutdownSignal,
    handle_events: impl Fn(&mut [Event], Option<Bytes>, usize) + Clone + Send + Sync + 'static,
    out: Pipeline,
) -> Result<(), ()> {
    let mut out = out.sink_map_err(|error| error!(message = "Error sending line.", %error));
    let mut buf = BytesMut::with_capacity(max_length);
    loop {
        buf.resize(max_length, 0);
        tokio::select! {
            recv = socket.recv_from(&mut buf) => {
                let (byte_size, address) = recv.map_err(|error| {
                    let error = codecs::decoding::Error::FramingError(error.into());
                    emit!(&SocketReceiveError {
                        mode: SocketMode::Unix,
                        error: &error
                    })
                })?;

                let payload = buf.split_to(byte_size);

                let span = info_span!("datagram");
                let path = address.as_pathname().map(|e| e.to_owned()).map(|path| {
                    span.record("peer_path", &field::debug(&path));
                    path
                });

                let received_from: Option<Bytes> =
                    path.map(|p| p.to_string_lossy().into_owned().into());

                let mut stream = FramedRead::new(payload.as_ref(), decoder.clone());

                loop {
                    match stream.next().await {
                        Some(Ok((mut events, byte_size))) => {
                            handle_events(&mut events, received_from.clone(), byte_size);

                            for event in events {
                                out.send(event).await?;
                            }
                        },
                        Some(Err(error)) => {
                            emit!(&SocketReceiveError {
                                mode: SocketMode::Unix,
                                error: &error
                            });
                            if !error.can_continue() {
                                break;
                            }
                        },
                        None => break,
                    }
                }
            }
            _ = &mut shutdown => return Ok(()),
        }
    }
}
