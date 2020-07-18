use crate::{
    event::{self, Event},
    internal_events::{UdpEventReceived, UdpSocketError},
    shutdown::ShutdownSignal,
    sources::Source,
};
use codec::BytesDelimitedCodec;
use futures::{
    compat::{Future01CompatExt, Sink01CompatExt},
    FutureExt, StreamExt, TryFutureExt,
};
use futures01::{sync::mpsc, Sink};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use string_cache::DefaultAtom as Atom;
use tokio::net::UdpSocket;
use tokio_util::udp::UdpFramed;

/// UDP processes messages per packet, where messages are separated by newline.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UdpConfig {
    pub address: SocketAddr,
    pub host_key: Option<Atom>,
}

impl UdpConfig {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            host_key: None,
        }
    }
}

pub fn udp(
    address: SocketAddr,
    host_key: Atom,
    shutdown: ShutdownSignal,
    out: mpsc::Sender<Event>,
) -> Source {
    let out = out.sink_map_err(|e| error!("error sending event: {:?}", e));

    Box::new(
        async move {
            let socket = UdpSocket::bind(&address)
                .await
                .expect("failed to bind to udp listener socket");
            info!(message = "listening.", %address);

            let _ = UdpFramed::new(socket, BytesDelimitedCodec::new(b'\n'))
                .take_until(shutdown.compat())
                .filter_map(|frame| {
                    let host_key = host_key.clone();
                    async move {
                        match frame {
                            Ok((line, addr)) => {
                                let byte_size = line.len();
                                let mut event = Event::from(line);

                                event
                                    .as_mut_log()
                                    .insert(event::log_schema().source_type_key(), "socket");

                                event.as_mut_log().insert(host_key, addr.to_string());

                                emit!(UdpEventReceived { byte_size });
                                Some(Ok(event))
                            }
                            Err(error) => {
                                emit!(UdpSocketError { error });
                                None
                            }
                        }
                    }
                })
                .forward(out.sink_compat())
                .await;

            Ok(())
        }
        .boxed()
        .compat(),
    )
}
