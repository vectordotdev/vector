use crate::{
    event::{self, Event},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use codec::BytesDelimitedCodec;
use futures::{future, sync::mpsc, Async, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::{io, net::SocketAddr};
use string_cache::DefaultAtom as Atom;
use tokio::{
    codec::{BytesCodec, Decoder},
    net::{UdpFramed, UdpSocket},
    prelude::stream::poll_fn,
};
use tracing::field;

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

#[typetag::serde(name = "udp")]
impl SourceConfig for UdpConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> Result<super::Source, String> {
        let host_key = self.host_key.clone().unwrap_or(event::HOST.clone());
        Ok(udp(self.address, host_key, out))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

pub fn udp(address: SocketAddr, host_key: Atom, out: mpsc::Sender<Event>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending event: {:?}", e));

    Box::new(
        future::lazy(move || {
            let socket = UdpSocket::bind(&address).expect("failed to bind to udp listener socket");

            info!(message = "listening.", %address);

            Ok(socket)
        })
        .and_then(move |socket| {
            let host_key = host_key.clone();
            let mut new_line_decoder = BytesDelimitedCodec::new(b'\n');
            UdpFramed::new(socket, BytesCodec::new())
                .map(move |(mut bytes, addr)| {
                    // A packet has come.
                    // UDP processes messages per packet, where messages are separated by newline.
                    let host_key = host_key.clone();
                    poll_fn(move || {
                        let maybe_event = match new_line_decoder.decode_eof(&mut bytes) {
                            Ok(Some(line)) => {
                                // One message
                                let mut event = Event::from(line);

                                event.as_mut_log().insert_implicit(
                                    host_key.clone().into(),
                                    addr.to_string().into(),
                                );

                                trace!(
                                    message = "Received one event.",
                                    event = field::debug(&event)
                                );
                                Some(event)
                            }
                            Ok(None) => None,
                            Err(error) => {
                                // Even if an Error occures, it should not be propagated further as it
                                // only affects this datagram.
                                error!(message = "error decoding datagram.", %error);
                                None
                            }
                        };
                        Ok(Async::Ready(maybe_event))
                    })
                })
                // Flatten messages from single packet
                .flatten()
                // Error from UdpSocket
                .map_err(|error: io::Error| error!(message = "error reading datagram.", %error))
                .forward(out)
                // Done with listening and sending
                .map(|_| ())
        }),
    )
}

#[cfg(test)]
mod test {
    use super::UdpConfig;
    use crate::event;
    use crate::test_util::{collect_n, next_addr};
    use crate::topology::config::{GlobalOptions, SourceConfig};
    use futures::sync::mpsc;
    use std::{
        net::{SocketAddr, UdpSocket},
        thread,
        time::Duration,
    };

    fn send_lines<'a>(addr: SocketAddr, lines: impl IntoIterator<Item = &'a str>) -> SocketAddr {
        let bind = next_addr();

        let socket = UdpSocket::bind(bind)
            .map_err(|e| panic!("{:}", e))
            .ok()
            .unwrap();

        for line in lines {
            assert_eq!(
                socket
                    .send_to(line.as_bytes(), addr)
                    .map_err(|e| panic!("{:}", e))
                    .ok()
                    .unwrap(),
                line.as_bytes().len()
            );
            // Space things out slightly to try to avoid dropped packets
            thread::sleep(Duration::from_millis(1));
        }

        // Give packets some time to flow through
        thread::sleep(Duration::from_millis(10));

        // Done
        bind
    }

    fn init_udp(sender: mpsc::Sender<event::Event>) -> (SocketAddr, tokio::runtime::Runtime) {
        let addr = next_addr();

        let server = UdpConfig::new(addr)
            .build("default", &GlobalOptions::default(), sender)
            .unwrap();
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(server);

        // Wait for udp to start listening
        thread::sleep(Duration::from_millis(100));

        (addr, rt)
    }

    #[test]
    fn udp_message() {
        let (tx, rx) = mpsc::channel(2);

        let (address, mut rt) = init_udp(tx);

        send_lines(address, vec!["test"]);
        let events = rt.block_on(collect_n(rx, 1)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::MESSAGE], "test".into());
    }

    #[test]
    fn udp_multiple_messages() {
        let (tx, rx) = mpsc::channel(10);

        let (address, mut rt) = init_udp(tx);

        send_lines(address, vec!["test\ntest2"]);
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::MESSAGE], "test".into());
        assert_eq!(events[1].as_log()[&event::MESSAGE], "test2".into());
    }

    #[test]
    fn udp_multiple_packets() {
        let (tx, rx) = mpsc::channel(10);

        let (address, mut rt) = init_udp(tx);

        send_lines(address, vec!["test", "test2"]);
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::MESSAGE], "test".into());
        assert_eq!(events[1].as_log()[&event::MESSAGE], "test2".into());
    }

    #[test]
    fn udp_it_includes_host() {
        let (tx, rx) = mpsc::channel(2);

        let (address, mut rt) = init_udp(tx);

        let from = send_lines(address, vec!["test"]);
        let events = rt.block_on(collect_n(rx, 1)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::HOST], format!("{}", from).into());
    }

}
