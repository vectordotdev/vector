use std::{
    fs::remove_file,
    path::PathBuf,
    time::Duration,
};

use codecs::StreamDecodingError;
use futures::{FutureExt, StreamExt};
use tokio::{
    io::AsyncWriteExt,
    net::{UnixListener, UnixStream},
    time::sleep,
};
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::codec::FramedRead;
use tracing::{field, Instrument};
use vector_common::internal_event::{ByteSize, BytesReceived, InternalEventHandle as _, Protocol};
use vector_core::EstimatedJsonEncodedSizeOf;

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
    sources::util::unix::{UnixSocketMetadata,UnixSocketMetadataCollectTypes, get_socket_inode},
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
    collect_metadata: UnixSocketMetadataCollectTypes,
    decoder: Decoder,
    handle_events: impl Fn(&mut [Event], &UnixSocketMetadata) + Clone + Send + Sync + 'static,
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

            let socket_metadata = get_socket_metadata(&socket, collect_metadata).await;

            let span = info_span!("connection");
            span.record("peer_path", field::debug(socket_metadata.peer_path_or_default()));

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

                                handle_events(&mut events, &socket_metadata);

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

// This method gets all the metadata we can about the socket. It unconditionally returns
// a UnixSocketMetadata object containing everything we found out about it through various
// system calls (which could be nothing - each of the _fields_ in UnixSocketMetadata is
// an Optional).
async fn get_socket_metadata(
    socket: &tokio::net::UnixStream,
    collect_metadata: UnixSocketMetadataCollectTypes,
) -> UnixSocketMetadata {
    // First thing to try - use getpeername(2) to see if the associated socket has a name.
    let peer_path = if collect_metadata.peer_path {
        socket
            .peer_addr()
            .map_err(|error| {
                // Log & throw error away
                debug!(message = "failed to get socket peer address.", %error);
                ()
            })
            .ok()
            .and_then(|addr| {
                addr.as_pathname().map(|p| { p.to_owned() })
            })
            .map(|path| -> String {
                path.to_string_lossy().into()
            })
    } else {
        None
    };

    // Try using fstat(2) to get the socket inode number
    let socket_inode = if collect_metadata.socket_inode {
        match get_socket_inode(socket).await {
            Err(error) => {
                debug!(message = "failed to get socket inode.", %error);
                None
            },
            Ok(inode) => Some(inode),
        }
    } else {
        None
    };

    UnixSocketMetadata{
        peer_path,
        socket_inode,
    }
}
