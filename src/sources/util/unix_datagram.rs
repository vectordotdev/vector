use std::{fs::remove_file, path::PathBuf};

use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use tokio::net::UnixDatagram;
use tokio_util::codec::FramedRead;
use tracing::field;
use vector_lib::codecs::StreamDecodingError;
use vector_lib::internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol};
use vector_lib::EstimatedJsonEncodedSizeOf;

use crate::{
    codecs::Decoder,
    event::Event,
    internal_events::{
        SocketEventsReceived, SocketMode, SocketReceiveError, StreamClosedError,
        UnixSocketFileDeleteError,
    },
    shutdown::ShutdownSignal,
    sources::util::change_socket_permissions,
    sources::util::unix::UNNAMED_SOCKET_HOST,
    sources::Source,
    SourceSender,
};

/// Returns a `Source` object corresponding to a Unix domain datagram socket.
/// Passing in different functions for `decoder` and `handle_events` can allow
/// for different source-specific logic (such as decoding syslog messages in the
/// syslog source).
pub fn build_unix_datagram_source(
    listen_path: PathBuf,
    socket_file_mode: Option<u32>,
    max_length: usize,
    decoder: Decoder,
    handle_events: impl Fn(&mut [Event], Option<Bytes>) + Clone + Send + Sync + 'static,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<Source> {
    Ok(Box::pin(async move {
        let socket = UnixDatagram::bind(&listen_path).expect("Failed to bind to datagram socket");
        info!(message = "Listening.", path = ?listen_path, r#type = "unix_datagram");

        change_socket_permissions(&listen_path, socket_file_mode)
            .expect("Failed to set socket permissions");

        let result = listen(socket, max_length, decoder, shutdown, handle_events, out).await;

        // Delete socket file.
        if let Err(error) = remove_file(&listen_path) {
            emit!(UnixSocketFileDeleteError {
                path: &listen_path,
                error
            });
        }

        result
    }))
}

async fn listen(
    socket: UnixDatagram,
    max_length: usize,
    decoder: Decoder,
    mut shutdown: ShutdownSignal,
    handle_events: impl Fn(&mut [Event], Option<Bytes>) + Clone + Send + Sync + 'static,
    mut out: SourceSender,
) -> Result<(), ()> {
    let mut buf = BytesMut::with_capacity(max_length);
    let bytes_received = register!(BytesReceived::from(Protocol::UNIX));
    loop {
        buf.resize(max_length, 0);
        tokio::select! {
            recv = socket.recv_from(&mut buf) => {
                let (byte_size, address) = recv.map_err(|error| {
                    let error = vector_lib::codecs::decoding::Error::FramingError(error.into());
                    emit!(SocketReceiveError {
                        mode: SocketMode::Unix,
                        error: &error
                    })
                })?;

                let span = info_span!("datagram");
                let received_from = if !address.is_unnamed() {
                    let path = address.as_pathname().map(|e| e.to_owned()).map(|path| {
                        span.record("peer_path", &field::debug(&path));
                        path
                    });

                    path.map(|p| p.to_string_lossy().into_owned().into())
                } else {
                    // In most cases, we'll be connecting to this
                    // socket from an unnamed socket (a socket not
                    // bound to a file). Instead of a filename, we'll
                    // surface a specific host value.
                    span.record("peer_path", &field::debug(UNNAMED_SOCKET_HOST));
                    Some(UNNAMED_SOCKET_HOST.into())
                };

                bytes_received.emit(ByteSize(byte_size));

                let payload = buf.split_to(byte_size);

                let mut stream = FramedRead::new(payload.as_ref(), decoder.clone());

                loop {
                    match stream.next().await {
                        Some(Ok((mut events, _byte_size))) => {
                            emit!(SocketEventsReceived {
                                mode: SocketMode::Unix,
                                byte_size: events.estimated_json_encoded_size_of(),
                                count: events.len()
                            });

                            handle_events(&mut events, received_from.clone());

                            let count = events.len();
                            if (out.send_batch(events).await).is_err() {
                                emit!(StreamClosedError { count });
                            }
                        },
                        Some(Err(error)) => {
                            emit!(SocketReceiveError {
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
