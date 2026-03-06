use std::pin::Pin;

use bytes::{Bytes, BytesMut};
use futures::{StreamExt, stream::BoxStream};
use futures_util::stream::Peekable;
#[cfg(unix)]
use std::path::PathBuf;
use tokio::net::UdpSocket;
#[cfg(unix)]
use tokio::net::UnixDatagram;
use tokio_util::codec::Encoder;
use vector_lib::{
    codecs::encoding::{Chunker, Chunking},
    internal_event::{ByteSize, BytesSent, InternalEventHandle, RegisterInternalEvent},
};

#[cfg(unix)]
use crate::internal_events::{UnixSendIncompleteError, UnixSocketSendError};
use crate::{
    codecs::Transformer,
    event::{Event, EventFinalizers, EventStatus, Finalizable},
    internal_events::{
        SocketEventsSent, SocketMode, SocketSendError, UdpChunkingError, UdpSendIncompleteError,
    },
};

pub enum DatagramSocket {
    Udp(UdpSocket),
    #[cfg(unix)]
    Unix(UnixDatagram, PathBuf),
}

/// A pre-encoded datagram ready to be sent over the socket.
pub struct EncodedDatagram {
    /// The encoded bytes to send (`None` if encoding failed).
    pub bytes: Option<Bytes>,
    pub finalizers: EventFinalizers,
}

enum SendOutcome {
    /// Datagram was successfully sent.
    Delivered,
    /// Per-event error that reconnecting the socket cannot fix (e.g. EMSGSIZE,
    /// EDESTADDRREQ). Drop the event and move on.
    UnrecoverableError,
    /// Socket-level error that may be resolved by reconnecting.
    SocketError,
}

/// Returns `true` only for errors that are known to be transient socket-level
/// failures where reconnecting may succeed. All other errors — including
/// EDESTADDRREQ (os error 89), EMSGSIZE, and any unknown error — are treated as
/// non-recoverable so the stream can drop the event and make progress.
fn is_recoverable_socket_error(error: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    matches!(
        error.kind(),
        ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
            | ErrorKind::NetworkDown
            | ErrorKind::HostUnreachable
            | ErrorKind::NetworkUnreachable
            | ErrorKind::TimedOut
            | ErrorKind::Interrupted
    )
}

/// Transforms and encodes a raw event stream into a stream of [`EncodedDatagram`]s
/// ready to be passed to [`send_datagrams`].
pub fn encode_to_datagrams<'a, E>(
    input: BoxStream<'a, Event>,
    transformer: Transformer,
    mut encoder: E,
) -> Peekable<BoxStream<'a, EncodedDatagram>>
where
    E: Encoder<Event, Error = vector_lib::codecs::encoding::Error> + Send + 'a,
{
    input
        .map(move |mut event| {
            transformer.transform(&mut event);
            let finalizers = event.take_finalizers();
            let mut bytes = BytesMut::new();
            let bytes = if encoder.encode(event, &mut bytes).is_ok() {
                Some(bytes.freeze())
            } else {
                None
            };
            EncodedDatagram { bytes, finalizers }
        })
        .boxed()
        .peekable()
}

pub async fn send_datagrams(
    input: &mut Peekable<BoxStream<'_, EncodedDatagram>>,
    mut socket: DatagramSocket,
    chunker: &Option<Chunker>,
    bytes_sent: &<BytesSent as RegisterInternalEvent>::Handle,
) {
    loop {
        // Peek without consuming so the event can be retried after reconnection.
        // Clone the bytes (ref-counted, cheap) to release the borrow on `input`.
        let Some(datagram) = Pin::new(&mut *input).peek().await else {
            break;
        };
        let bytes = datagram.bytes.clone();

        let Some(bytes) = bytes else {
            // Encoding failed earlier — consume and mark errored.
            if let Some(datagram) = input.next().await {
                datagram.finalizers.update_status(EventStatus::Errored);
            }
            continue;
        };

        let outcome = if let Some(chunker) = chunker {
            let data_size = bytes.len();
            match chunker.chunk(bytes) {
                Ok(chunks) => {
                    let mut result = SendOutcome::Delivered;
                    for chunk in chunks {
                        result = send_and_emit(&mut socket, &chunk, bytes_sent).await;
                        if !matches!(result, SendOutcome::Delivered) {
                            break;
                        }
                    }
                    result
                }
                Err(err) => {
                    emit!(UdpChunkingError {
                        data_size,
                        error: err
                    });
                    SendOutcome::UnrecoverableError
                }
            }
        } else {
            send_and_emit(&mut socket, &bytes, bytes_sent).await
        };

        match outcome {
            SendOutcome::Delivered => {
                if let Some(datagram) = input.next().await {
                    datagram.finalizers.update_status(EventStatus::Delivered);
                }
            }
            SendOutcome::SocketError => {
                // Leave item in stream for retry after reconnection.
                break;
            }
            SendOutcome::UnrecoverableError => {
                // Per-event or permanent error — consume and mark errored so the
                // stream can make progress rather than retrying forever.
                if let Some(datagram) = input.next().await {
                    datagram.finalizers.update_status(EventStatus::Errored);
                }
            }
        }
    }
}

async fn send_and_emit(
    socket: &mut DatagramSocket,
    bytes: &bytes::Bytes,
    bytes_sent: &<BytesSent as RegisterInternalEvent>::Handle,
) -> SendOutcome {
    match send_datagram(socket, bytes).await {
        Ok(()) => {
            emit!(SocketEventsSent {
                mode: match socket {
                    DatagramSocket::Udp(_) => SocketMode::Udp,
                    #[cfg(unix)]
                    DatagramSocket::Unix(..) => SocketMode::Unix,
                },
                count: 1,
                byte_size: bytes.len().into(),
            });
            bytes_sent.emit(ByteSize(bytes.len()));
            SendOutcome::Delivered
        }
        Err(error) => {
            let recoverable = is_recoverable_socket_error(&error);
            match socket {
                DatagramSocket::Udp(_) => emit!(SocketSendError {
                    mode: SocketMode::Udp,
                    error
                }),
                #[cfg(unix)]
                DatagramSocket::Unix(_, path) => {
                    emit!(UnixSocketSendError {
                        path: path.as_path(),
                        error: &error
                    })
                }
            };
            if recoverable {
                SendOutcome::SocketError
            } else {
                SendOutcome::UnrecoverableError
            }
        }
    }
}

async fn send_datagram(socket: &mut DatagramSocket, buf: &[u8]) -> tokio::io::Result<()> {
    let sent = match socket {
        DatagramSocket::Udp(udp) => udp.send(buf).await,
        #[cfg(unix)]
        DatagramSocket::Unix(uds, _) => uds.send(buf).await,
    }?;
    if sent != buf.len() {
        match socket {
            DatagramSocket::Udp(_) => emit!(UdpSendIncompleteError {
                data_size: buf.len(),
                sent,
            }),
            #[cfg(unix)]
            DatagramSocket::Unix(..) => emit!(UnixSendIncompleteError {
                data_size: buf.len(),
                sent,
            }),
        }
    }
    Ok(())
}
