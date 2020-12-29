use crate::{
    config::log_schema,
    event::LookupBuf,
    internal_events::{SocketEventReceived, SocketMode, SocketReceiveError},
    log_event,
    shutdown::ShutdownSignal,
    sources::Source,
    Pipeline,
};
use bytes::{Bytes, BytesMut};
use codec::BytesDelimitedCodec;
use futures::SinkExt;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use tokio::net::UdpSocket;
use tokio_util::codec::Decoder;

/// UDP processes messages per packet, where messages are separated by newline.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UdpConfig {
    pub address: SocketAddr,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    pub host_key: Option<LookupBuf>,
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

impl UdpConfig {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            max_length: default_max_length(),
            host_key: None,
        }
    }
}

pub fn udp(
    address: SocketAddr,
    max_length: usize,
    host_key: LookupBuf,
    mut shutdown: ShutdownSignal,
    out: Pipeline,
) -> Source {
    let mut out = out.sink_map_err(|error| error!(message = "Error sending event.", %error));

    Box::pin(async move {
        let mut socket = UdpSocket::bind(&address)
            .await
            .expect("Failed to bind to udp listener socket");
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
                        let event = log_event! {
                            log_schema().message_key().clone() => line,
                            log_schema().timestamp_key().clone() => chrono::Utc::now(),
                            log_schema().source_type_key().clone() => Bytes::from("socket"),
                            host_key.clone() => address.to_string(),
                        };

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
