use std::pin::Pin;

use bytes::Bytes;
use futures::{StreamExt, stream::BoxStream};
use futures_util::stream::Peekable;
#[cfg(unix)]
use std::path::PathBuf;
use tokio::net::UdpSocket;
#[cfg(unix)]
use tokio::net::UnixDatagram;
use vector_lib::{
    codecs::encoding::{Chunker, Chunking},
    internal_event::{ByteSize, BytesSent, InternalEventHandle, RegisterInternalEvent},
};

#[cfg(unix)]
use crate::internal_events::{UnixSendIncompleteError, UnixSocketSendError};
use crate::{
    event::{EventFinalizers, EventStatus},
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

        let mut socket_error = false;
        let delivered = if let Some(chunker) = chunker {
            let data_size = bytes.len();
            match chunker.chunk(bytes) {
                Ok(chunks) => {
                    let mut chunks_delivered = true;
                    for chunk in chunks {
                        if !send_and_emit(&mut socket, &chunk, bytes_sent).await {
                            chunks_delivered = false;
                            socket_error = true;
                            break;
                        }
                    }
                    chunks_delivered
                }
                Err(err) => {
                    emit!(UdpChunkingError {
                        data_size,
                        error: err
                    });
                    false
                }
            }
        } else if send_and_emit(&mut socket, &bytes, bytes_sent).await {
            true
        } else {
            socket_error = true;
            false
        };

        if delivered {
            if let Some(datagram) = input.next().await {
                datagram.finalizers.update_status(EventStatus::Delivered);
            }
        } else if socket_error {
            // Socket error — leave item in stream for retry after reconnection.
            break;
        } else {
            // Chunking error — consume and mark errored, continue with next event.
            if let Some(datagram) = input.next().await {
                datagram.finalizers.update_status(EventStatus::Errored);
            }
        }
    }
}

async fn send_and_emit(
    socket: &mut DatagramSocket,
    bytes: &bytes::Bytes,
    bytes_sent: &<BytesSent as RegisterInternalEvent>::Handle,
) -> bool {
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
            true
        }
        Err(error) => {
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
            false
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
