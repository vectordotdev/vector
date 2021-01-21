use crate::{
    event::Event,
    internal_events::{SocketEventReceived, SocketMode, SocketReceiveError},
    shutdown::ShutdownSignal,
    sources::Source,
    Pipeline,
};
use bytes::{Bytes, BytesMut};
use codec::BytesDelimitedCodec;
use futures::SinkExt;
use getset::{CopyGetters, Getters};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket};
use tokio::net::UdpSocket;
use tokio_util::codec::Decoder;

/// UDP processes messages per packet, where messages are separated by newline.
#[derive(Deserialize, Serialize, Debug, Clone, Getters, CopyGetters)]
#[serde(deny_unknown_fields)]
pub struct UdpConfig {
    #[get_copy = "pub"]
    address: SocketAddr,
    #[serde(default = "default_max_length")]
    #[get_copy = "pub"]
    max_length: usize,
    #[get = "pub"]
    host_key: Option<String>,
    #[get_copy = "pub"]
    send_buffer_bytes: Option<usize>,
    #[get_copy = "pub"]
    receive_buffer_bytes: Option<usize>,
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

impl UdpConfig {
    pub fn from_address(address: SocketAddr) -> Self {
        Self {
            address,
            max_length: default_max_length(),
            host_key: None,
            send_buffer_bytes: None,
            receive_buffer_bytes: None,
        }
    }
}

pub fn udp(
    address: SocketAddr,
    max_length: usize,
    host_key: String,
    send_buffer_bytes: Option<usize>,
    receive_buffer_bytes: Option<usize>,
    mut shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    let mut out = out.sink_map_err(|error| error!(message = "Error sending event.", %error));

    Box::pin(async move {
        let mut socket = UdpSocket::bind(&address)
            .await
            .expect("Failed to bind to udp listener socket");
        info!(message = "Listening.", address = %address);

        {
            // SAFETY: We temporarily take ownership of the socket and return it by the end of this block scope.
            let socket = unsafe {
                #[cfg(unix)]
                {
                    socket2::Socket::from_raw_fd(socket.as_raw_fd())
                }
                #[cfg(windows)]
                {
                    socket2::Socket::from_raw_socket(socket.as_raw_socket())
                }
            };

            if let Some(send_buffer_bytes) = send_buffer_bytes {
                if let Err(error) = socket.set_send_buffer_size(send_buffer_bytes) {
                    warn!(message = "Failed configuring send buffer size on UDP socket.", %error);
                }
            }

            if let Some(receive_buffer_bytes) = receive_buffer_bytes {
                if let Err(error) = socket.set_recv_buffer_size(receive_buffer_bytes) {
                    warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
                }
            }

            #[cfg(unix)]
            socket.into_raw_fd();
            #[cfg(windows)]
            socket.into_raw_socket();
        }

        let max_length = if let Some(receive_buffer_bytes) = receive_buffer_bytes {
            std::cmp::min(max_length, receive_buffer_bytes)
        } else {
            max_length
        };

        let mut buf = BytesMut::with_capacity(max_length);
        loop {
            buf.resize(max_length, 0);
            tokio::select! {
                recv = socket.recv_from(&mut buf) => {
                    let (byte_size, address) = recv.map_err(|error| {
                        emit!(SocketReceiveError {
                            error,
                            mode: SocketMode::Udp
                        });
                    })?;

                    let mut payload = buf.split_to(byte_size);

                    // UDP processes messages per payload, where messages are separated by newline
                    // and stretch to end of payload.
                    let mut decoder = BytesDelimitedCodec::new(b'\n');
                    while let Ok(Some(line)) = decoder.decode_eof(&mut payload) {
                        let mut event = Event::from(line);

                        event
                            .as_mut_log()
                            .insert(crate::config::log_schema().source_type_key(), Bytes::from("socket"));
                        event
                            .as_mut_log()
                            .insert(host_key.clone(), address.to_string());

                        emit!(SocketEventReceived { byte_size,mode:SocketMode::Udp });

                        tokio::select!{
                            result = out.send(event) => {match result {
                                Ok(()) => { },
                                Err(()) => return Ok(()),
                            }}
                            _ = &mut shutdown => return Ok(()),
                        }
                    }
                }
                _ = &mut shutdown => return Ok(()),
            }
        }
    })
}
