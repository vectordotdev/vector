use bytes::BytesMut;
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
    event::{Event, EventStatus, Finalizable},
    internal_events::{
        SocketEventsSent, SocketMode, SocketSendError, UdpChunkingError, UdpSendIncompleteError,
    },
};

pub enum DatagramSocket {
    Udp(UdpSocket),
    #[cfg(unix)]
    Unix(UnixDatagram, PathBuf),
}

pub async fn send_datagrams<E: Encoder<Event, Error = vector_lib::codecs::encoding::Error>>(
    input: &mut Peekable<BoxStream<'_, Event>>,
    mut socket: DatagramSocket,
    transformer: &Transformer,
    encoder: &mut E,
    chunker: &Option<Chunker>,
    bytes_sent: &<BytesSent as RegisterInternalEvent>::Handle,
) {
    while let Some(mut event) = input.next().await {
        transformer.transform(&mut event);
        let finalizers = event.take_finalizers();
        let mut bytes = BytesMut::new();

        // Errors are handled by `Encoder`.
        if encoder.encode(event, &mut bytes).is_err() {
            finalizers.update_status(EventStatus::Errored);
            continue;
        }

        let delivered = if let Some(chunker) = chunker {
            let data_size = bytes.len();
            match chunker.chunk(bytes.freeze()) {
                Ok(chunks) => {
                    let mut chunks_delivered = true;
                    for bytes in chunks {
                        if !send_and_emit(&mut socket, &bytes, bytes_sent).await {
                            chunks_delivered = false;
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
        } else {
            send_and_emit(&mut socket, &bytes.freeze(), bytes_sent).await
        };

        if delivered {
            finalizers.update_status(EventStatus::Delivered);
        } else {
            finalizers.update_status(EventStatus::Errored);
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
