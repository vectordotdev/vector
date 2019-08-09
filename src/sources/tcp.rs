use super::util::TcpSource;
use crate::{
    event::{self, Event},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use bytes::Bytes;
use codec::{self, BytesDelimitedCodec};
use futures::sync::mpsc;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use string_cache::DefaultAtom as Atom;
use tracing::field;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct TcpConfig {
    pub address: SocketAddr,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
    pub host_key: Option<Atom>,
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl TcpConfig {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            max_length: default_max_length(),
            host_key: None,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
        }
    }
}

#[typetag::serde(name = "tcp")]
impl SourceConfig for TcpConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> Result<super::Source, String> {
        let tcp = RawTcpSource {
            config: self.clone(),
        };
        tcp.run(self.address, self.shutdown_timeout_secs, out)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

#[derive(Debug, Clone)]
struct RawTcpSource {
    config: TcpConfig,
}

impl TcpSource for RawTcpSource {
    type Decoder = BytesDelimitedCodec;

    fn decoder(&self) -> Self::Decoder {
        BytesDelimitedCodec::new_with_max_length(b'\n', self.config.max_length)
    }

    fn build_event(&self, frame: Bytes, host: Option<Bytes>) -> Option<Event> {
        let mut event = Event::from(frame);

        let host_key = if let Some(key) = &self.config.host_key {
            key
        } else {
            &event::HOST
        };

        if let Some(host) = host {
            event
                .as_mut_log()
                .insert_implicit(host_key.clone(), host.into());
        }

        trace!(
            message = "Received one event.",
            event = field::debug(&event)
        );
        Some(event)
    }
}

#[cfg(test)]
mod test {
    use super::TcpConfig;
    use crate::event;
    use crate::test_util::{block_on, next_addr, send_lines, wait_for_tcp};
    use crate::topology::config::{GlobalOptions, SourceConfig};
    use futures::sync::mpsc;
    use futures::Stream;

    #[test]
    fn tcp_it_includes_host() {
        let (tx, rx) = mpsc::channel(1);

        let addr = next_addr();

        let server = TcpConfig::new(addr)
            .build("default", &GlobalOptions::default(), tx)
            .unwrap();
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(server);
        wait_for_tcp(addr);

        rt.block_on(send_lines(addr, vec!["test".to_owned()].into_iter()))
            .unwrap();

        let event = rx.wait().next().unwrap().unwrap();
        assert_eq!(event.as_log()[&event::HOST], "127.0.0.1".into());
    }

    #[test]
    fn tcp_it_defaults_max_length() {
        let with: super::TcpConfig = toml::from_str(
            r#"
            address = "127.0.0.1:1234"
            max_length = 19
            "#,
        )
        .unwrap();

        let without: super::TcpConfig = toml::from_str(
            r#"
            address = "127.0.0.1:1234"
            "#,
        )
        .unwrap();

        assert_eq!(with.max_length, 19);
        assert_eq!(without.max_length, super::default_max_length());
    }

    #[test]
    fn tcp_continue_after_long_line() {
        let (tx, rx) = mpsc::channel(10);

        let addr = next_addr();

        let mut config = TcpConfig::new(addr);
        config.max_length = 10;

        let server = config
            .build("default", &GlobalOptions::default(), tx)
            .unwrap();
        let mut rt = tokio::runtime::Runtime::new().unwrap();
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
}
