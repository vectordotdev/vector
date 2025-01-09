#[cfg(unix)]
use std::path::PathBuf;

use bytes::BytesMut;
use futures::{stream::BoxStream, StreamExt};
use futures_util::stream::Peekable;
use tokio::net::UdpSocket;
#[cfg(unix)]
use tokio::net::UnixDatagram;
use tokio_util::codec::Encoder;
use vector_lib::internal_event::RegisterInternalEvent;
use vector_lib::internal_event::{ByteSize, BytesSent, InternalEventHandle};
use vector_lib::EstimatedJsonEncodedSizeOf;

use crate::{
    codecs::Transformer,
    event::{Event, EventStatus, Finalizable},
    internal_events::{SocketEventsSent, SocketMode, SocketSendError, UdpSendIncompleteError},
};

#[cfg(unix)]
use crate::internal_events::{UnixSendIncompleteError, UnixSocketSendError};

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
    bytes_sent: &<BytesSent as RegisterInternalEvent>::Handle,
) {
    while let Some(mut event) = input.next().await {
        let byte_size = event.estimated_json_encoded_size_of();

        transformer.transform(&mut event);

        let finalizers = event.take_finalizers();
        let mut bytes = BytesMut::new();

        // Errors are handled by `Encoder`.
        if encoder.encode(event, &mut bytes).is_err() {
            continue;
        }

        match send_datagram(&mut socket, &bytes).await {
            Ok(()) => {
                emit!(SocketEventsSent {
                    mode: match socket {
                        DatagramSocket::Udp(_) => SocketMode::Udp,
                        #[cfg(unix)]
                        DatagramSocket::Unix(..) => SocketMode::Unix,
                    },
                    count: 1,
                    byte_size,
                });

                bytes_sent.emit(ByteSize(bytes.len()));
                finalizers.update_status(EventStatus::Delivered);
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
                finalizers.update_status(EventStatus::Errored);
                return;
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
