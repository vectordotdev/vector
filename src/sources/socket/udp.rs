use crate::{
    event::{self, Event},
    internal_events::{UdpEventReceived, UdpSocketError},
    shutdown::ShutdownSignal,
};
use bytes::BytesMut;
use codec::BytesDelimitedCodec;
use futures::compat::Future01CompatExt;
use futures01::{sync::mpsc, Sink};
use serde::{Deserialize, Serialize};
use std::{io, net::SocketAddr};
use string_cache::DefaultAtom as Atom;
use tokio::net::UdpSocket;
use tokio::select;
use tokio_codec::Decoder;

/// UDP processes messages per packet, where messages are separated by newline.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UdpConfig {
    pub address: SocketAddr,
    pub host_key: Option<Atom>,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
}

impl UdpConfig {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            host_key: None,
            max_length: default_max_length(),
        }
    }
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

pub async fn udp(
    address: SocketAddr,
    max_length: usize,
    host_key: Atom,
    shutdown: ShutdownSignal,
    mut out: mpsc::Sender<Event>,
) -> Result<(), ()> {
    let mut socket = UdpSocket::bind(&address)
        .await
        .map_err(|error| error!(message = "Failed to bind to udp listener socket.",%error))?;

    info!(message = "listening.", %address);

    let host_key = host_key.clone();

    // Buffer for accepting udp payload.
    let mut buffer = BytesMut::with_capacity(max_length);
    buffer.resize(max_length, 0);

    let mut shutdown = shutdown.compat();
    'main: loop {
        select! {
            udp_result = socket.recv_from(&mut buffer) => {
                let (byte_size, address) = udp_result.map_err(|error: io::Error| {
                    emit!(UdpSocketError { error });
                })?;

                let mut payload = buffer.split_to(byte_size);

                // UDP processes messages per payload, where messages are separated by newline
                // and stretch to end of payload.
                let mut decoder = BytesDelimitedCodec::new(b'\n');
                while let Ok(Some(line)) = decoder.decode_eof(&mut payload) {
                    let mut event = Event::from(line);

                    event
                        .as_mut_log()
                        .insert(event::log_schema().source_type_key(), "socket");
                    event
                        .as_mut_log()
                        .insert(host_key.clone(), address.to_string());

                    emit!(UdpEventReceived { byte_size });

                    select!{
                        result = out.send(event).compat() => {
                            out=result.map_err(|error| error!(message = "Error sending event.", %error))?;
                        }
                        _ = &mut shutdown => break 'main,
                    }
                }

                buffer.reserve(byte_size);
                buffer.resize(max_length, 0);
            }
            _ = &mut shutdown => break,
        }
    }

    Ok(())
}
