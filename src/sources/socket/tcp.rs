use crate::{
    event::{self, Event},
    internal_events::TcpEventReceived,
    sources::util::{SocketListenAddr, TcpSource},
    tls::TlsConfig,
};
use bytes::Bytes;
use codec::{self, BytesDelimitedCodec};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;
use tracing::field;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct TcpConfig {
    pub address: SocketListenAddr,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
    pub host_key: Option<Atom>,
    pub tls: Option<TlsConfig>,
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl TcpConfig {
    pub fn new(address: SocketListenAddr) -> Self {
        Self {
            address,
            max_length: default_max_length(),
            host_key: None,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            tls: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawTcpSource {
    pub config: TcpConfig,
}

impl TcpSource for RawTcpSource {
    type Decoder = BytesDelimitedCodec;

    fn decoder(&self) -> Self::Decoder {
        BytesDelimitedCodec::new_with_max_length(b'\n', self.config.max_length)
    }

    fn build_event(&self, frame: Bytes, host: Bytes) -> Option<Event> {
        let byte_size = frame.len();
        let mut event = Event::from(frame);

        let host_key = if let Some(key) = &self.config.host_key {
            key
        } else {
            &event::log_schema().host_key()
        };

        event.as_mut_log().insert(host_key.clone(), host);

        trace!(
            message = "Received one event.",
            event = field::debug(&event)
        );
        emit!(TcpEventReceived { byte_size });

        Some(event)
    }
}

#[cfg(test)]
mod test {

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
}
