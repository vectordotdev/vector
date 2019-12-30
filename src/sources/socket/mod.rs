mod tcp;
mod udp;

use super::util::TcpSource;
use crate::{
    event::{self, Event},
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use futures::sync::mpsc;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Deserialize, Serialize, Debug, Clone)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct SocketConfig {
    #[serde(flatten)]
    pub mode: Mode,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp(tcp::TcpConfig),
    Udp(udp::UdpConfig),
}

impl SocketConfig {
    pub fn make_tcp_config(addr: SocketAddr) -> Self {
        tcp::TcpConfig::new(addr.into()).into()
    }
}

impl From<tcp::TcpConfig> for SocketConfig {
    fn from(config: tcp::TcpConfig) -> Self {
        SocketConfig {
            mode: Mode::Tcp(config),
        }
    }
}

impl From<udp::UdpConfig> for SocketConfig {
    fn from(config: udp::UdpConfig) -> Self {
        SocketConfig {
            mode: Mode::Udp(config),
        }
    }
}

inventory::submit! {
    SourceDescription::new_without_default::<SocketConfig>("socket")
}

#[typetag::serde(name = "socket")]
impl SourceConfig for SocketConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        match self.mode.clone() {
            Mode::Tcp(config) => {
                let tcp = tcp::RawTcpSource {
                    config: config.clone(),
                };
                tcp.run(config.address, config.shutdown_timeout_secs, out)
            }
            Mode::Udp(config) => {
                let host_key = config.host_key.clone().unwrap_or(event::HOST.clone());
                Ok(udp::udp(config.address, host_key, out))
            }
        }
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "socket"
    }
}

#[cfg(test)]
mod test {
    use super::tcp::TcpConfig;
    use super::udp::UdpConfig;
    use super::SocketConfig;
    use crate::event;
    use crate::runtime;
    use crate::test_util::{block_on, collect_n, next_addr, send_lines, wait_for_tcp};
    use crate::topology::config::{GlobalOptions, SourceConfig};
    use futures::sync::mpsc;
    use futures::Stream;
    use std::{
        net::{SocketAddr, UdpSocket},
        thread,
        time::Duration,
    };

    //////// TCP TESTS ////////
    #[test]
    fn tcp_it_includes_host() {
        let (tx, rx) = mpsc::channel(1);

        let addr = next_addr();

        let server = SocketConfig::from(TcpConfig::new(addr.into()))
            .build("default", &GlobalOptions::default(), tx)
            .unwrap();
        let mut rt = runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        rt.block_on(send_lines(addr, vec!["test".to_owned()].into_iter()))
            .unwrap();

        let event = rx.wait().next().unwrap().unwrap();
        assert_eq!(event.as_log()[&event::HOST], "127.0.0.1".into());
    }

    #[test]
    fn tcp_continue_after_long_line() {
        let (tx, rx) = mpsc::channel(10);

        let addr = next_addr();

        let mut config = TcpConfig::new(addr.into());
        config.max_length = 10;

        let server = SocketConfig::from(config)
            .build("default", &GlobalOptions::default(), tx)
            .unwrap();
        let mut rt = runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        let lines = vec![
            "short".to_owned(),
            "this is too long".to_owned(),
            "more short".to_owned(),
        ];

        rt.block_on(send_lines(addr, lines.into_iter())).unwrap();

        let (event, rx) = block_on(rx.into_future()).unwrap();
        assert_eq!(event.unwrap().as_log()[&event::MESSAGE], "short".into());

        let (event, _rx) = block_on(rx.into_future()).unwrap();
        assert_eq!(
            event.unwrap().as_log()[&event::MESSAGE],
            "more short".into()
        );
    }

    //////// UDP TESTS ////////
    fn send_lines_udp<'a>(
        addr: SocketAddr,
        lines: impl IntoIterator<Item = &'a str>,
    ) -> SocketAddr {
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

    fn init_udp(sender: mpsc::Sender<event::Event>) -> (SocketAddr, runtime::Runtime) {
        let addr = next_addr();

        let server = SocketConfig::from(UdpConfig::new(addr))
            .build("default", &GlobalOptions::default(), sender)
            .unwrap();
        let mut rt = runtime::Runtime::new().unwrap();
        rt.spawn(server);

        // Wait for udp to start listening
        thread::sleep(Duration::from_millis(100));

        (addr, rt)
    }

    #[test]
    fn udp_message() {
        let (tx, rx) = mpsc::channel(2);

        let (address, mut rt) = init_udp(tx);

        send_lines_udp(address, vec!["test"]);
        let events = rt.block_on(collect_n(rx, 1)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::MESSAGE], "test".into());
    }

    #[test]
    fn udp_multiple_messages() {
        let (tx, rx) = mpsc::channel(10);

        let (address, mut rt) = init_udp(tx);

        send_lines_udp(address, vec!["test\ntest2"]);
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::MESSAGE], "test".into());
        assert_eq!(events[1].as_log()[&event::MESSAGE], "test2".into());
    }

    #[test]
    fn udp_multiple_packets() {
        let (tx, rx) = mpsc::channel(10);

        let (address, mut rt) = init_udp(tx);

        send_lines_udp(address, vec!["test", "test2"]);
        let events = rt.block_on(collect_n(rx, 2)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::MESSAGE], "test".into());
        assert_eq!(events[1].as_log()[&event::MESSAGE], "test2".into());
    }

    #[test]
    fn udp_it_includes_host() {
        let (tx, rx) = mpsc::channel(2);

        let (address, mut rt) = init_udp(tx);

        let from = send_lines_udp(address, vec!["test"]);
        let events = rt.block_on(collect_n(rx, 1)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::HOST], format!("{}", from).into());
    }
}
