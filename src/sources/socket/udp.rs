use crate::udp;
use crate::{
    config::log_schema,
    event::Event,
    internal_events::{SocketEventReceived, SocketMode, SocketReceiveError},
    shutdown::ShutdownSignal,
    sources::Source,
    Pipeline,
};
use bytes::{Bytes, BytesMut};
use futures::SinkExt;
use getset::{CopyGetters, Getters};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
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

pub fn udp<D>(
    address: SocketAddr,
    max_length: usize,
    host_key: String,
    receive_buffer_bytes: Option<usize>,
    mut decoder: D,
    mut shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source
where
    D: Decoder<Item = (Event, usize)> + Send + 'static,
    D::Error: From<std::io::Error> + std::fmt::Debug + std::fmt::Display + Send,
{
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

        let max_length = if let Some(receive_buffer_bytes) = receive_buffer_bytes {
            std::cmp::min(max_length, receive_buffer_bytes)
        } else {
            max_length
        };

        info!(message = "Listening.", address = %address);

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

                    while let Ok(Some((mut event, byte_size))) = decoder.decode_eof(&mut payload) {
                        match event {
                            Event::Log(ref mut log) => {
                                log.insert(log_schema().source_type_key(), Bytes::from("socket"));
                                log.insert(host_key.clone(), address.to_string());
                            },
                            Event::Metric(_) => {}
                        }

                        emit!(SocketEventReceived { byte_size, mode: SocketMode::Udp });

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
