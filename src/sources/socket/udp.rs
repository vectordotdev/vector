use crate::{
    codecs::Decoder,
    event::Event,
    internal_events::SocketEventsReceived,
    shutdown::ShutdownSignal,
    socket::SocketMode,
    sources::{util::TcpError, Source},
    udp, Pipeline,
};
use async_stream::stream;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use getset::{CopyGetters, Getters};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio_util::udp::UdpFramed;

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
            receive_buffer_bytes: None,
        }
    }
}

pub fn udp(
    address: SocketAddr,
    host_key: String,
    receive_buffer_bytes: Option<usize>,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    let mut out = out.sink_map_err(|error| error!(message = "Error sending event.", %error));

    Box::pin(async move {
        let socket = UdpSocket::bind(&address)
            .await
            .expect("Failed to bind to udp listener socket");

        if let Some(receive_buffer_bytes) = receive_buffer_bytes {
            if let Err(error) = udp::set_receive_buffer_size(&socket, receive_buffer_bytes) {
                warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
            }
        }

        info!(message = "Listening.", address = %address);

        let mut stream = UdpFramed::new(socket, decoder).take_until(shutdown);
        (stream! {
            loop {
                match stream.next().await {
                    Some(Ok(((events, byte_size), received_from))) => {
                        emit!(SocketEventsReceived {
                            mode: SocketMode::Udp,
                            byte_size,
                            count: events.len()
                        });

                        for mut event in events {
                            if let Event::Log(ref mut log) = event {
                                log.insert(
                                    crate::config::log_schema().source_type_key(),
                                    Bytes::from("socket"),
                                );

                                log.insert(host_key.clone(), received_from.to_string());
                            }

                            yield event;
                        }
                    }
                    Some(Err(error)) => {
                        // Error is logged by `crate::codecs::Decoder`, no
                        // further handling is needed here.
                        if error.can_continue() {
                            continue;
                        } else {
                            break;
                        }
                    }
                    None => break,
                }
            }
        })
        .map(Ok)
        .forward(&mut out)
        .await
    })
}
