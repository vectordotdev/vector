use crate::udp;
use crate::{
    event::Event,
    internal_events::{SocketEventReceived, SocketMode, SocketReceiveError},
    shutdown::ShutdownSignal,
    sources::Source,
    Pipeline,
};
use bytes::Bytes;
use codec::BytesDelimitedCodec;
use futures::{SinkExt, StreamExt};
use getset::{CopyGetters, Getters, Setters};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio_util::{codec::BytesCodec, udp::UdpFramed};

/// UDP processes messages per packet, where messages are separated by newline.
#[derive(Deserialize, Serialize, Debug, Clone, Getters, CopyGetters, Setters)]
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
    receive_buffer_bytes: Option<usize>,
    #[getset(get_copy = "pub", set = "pub")]
    one_event_per_datagram: bool,
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
            receive_buffer_bytes: None,
            one_event_per_datagram: false,
        }
    }
}

pub fn udp(
    addr: SocketAddr,
    max_length: usize,
    host_key: String,
    receive_buffer_bytes: Option<usize>,
    one_event_per_datagram: bool,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    let out = out.sink_map_err(|error| error!(message = "Error sending event.", %error));

    Box::pin(async move {
        let socket = UdpSocket::bind(&addr)
            .await
            .expect("Failed to bind to UDP listener socket");

        if let Some(receive_buffer_bytes) = receive_buffer_bytes {
            if let Err(error) = udp::set_receive_buffer_size(&socket, receive_buffer_bytes) {
                warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
            }
        }

        info!(message = "Listening.", address = %addr);
        // UDP processes messages per payload, messages can be separated by newline
        // or each datagram can be used as a single message
        if one_event_per_datagram {
            let _ = UdpFramed::new(socket, BytesCodec::new())
                .take_until(shutdown)
                .filter_map(|frame| {
                    let host_key = host_key.clone();
                    async move {
                        match frame {
                            Ok((mut bytes, address)) => {
                                emit!(SocketEventReceived {
                                    byte_size: bytes.len(),
                                    mode: SocketMode::Udp
                                });
                                bytes.truncate(max_length);
                                Some(Ok(build_event(
                                    Bytes::from(bytes),
                                    address.to_string(),
                                    host_key,
                                )))
                            }
                            Err(error) => {
                                emit!(SocketReceiveError {
                                    error,
                                    mode: SocketMode::Udp
                                });
                                None
                            }
                        }
                    }
                })
                .forward(out)
                .await;
        } else {
            let _ = UdpFramed::new(socket, BytesDelimitedCodec::new(b'\n'))
                .take_until(shutdown)
                .filter_map(|frame| {
                    let host_key = host_key.clone();
                    async move {
                        match frame {
                            Ok((mut bytes, address)) => {
                                emit!(SocketEventReceived {
                                    byte_size: bytes.len(),
                                    mode: SocketMode::Udp
                                });
                                bytes.truncate(max_length);
                                Some(Ok(build_event(
                                    Bytes::from(bytes),
                                    address.to_string(),
                                    host_key,
                                )))
                            }
                            Err(error) => {
                                emit!(SocketReceiveError {
                                    error,
                                    mode: SocketMode::Udp
                                });
                                None
                            }
                        }
                    }
                })
                .forward(out)
                .await;
        }
        info!("Finished sending.");
        Ok(())
    })
}

fn build_event(content: Bytes, address: String, host_key: String) -> Event {
    let mut event = Event::from(content);
    event.as_mut_log().insert(
        crate::config::log_schema().source_type_key(),
        Bytes::from("socket"),
    );
    event.as_mut_log().insert(host_key.clone(), address);
    event
}
