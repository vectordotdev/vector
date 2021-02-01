#[cfg(unix)]
use crate::udp;
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
    #[cfg(unix)]
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
            #[cfg(unix)]
            receive_buffer_bytes: None,
        }
    }
}

pub fn udp(
    address: SocketAddr,
    max_length: usize,
    host_key: String,
    #[cfg(unix)] receive_buffer_bytes: Option<usize>,
    mut shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    let mut out = out.sink_map_err(|error| error!(message = "Error sending event.", %error));

    Box::pin(async move {
        let mut socket = UdpSocket::bind(&address)
            .await
            .expect("Failed to bind to udp listener socket");

        #[cfg(unix)]
        if let Some(receive_buffer_bytes) = receive_buffer_bytes {
            udp::set_receive_buffer_size(&socket, receive_buffer_bytes);
        }

        #[cfg(unix)]
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
