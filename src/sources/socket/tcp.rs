use crate::{
    event::Event,
    internal_events::{SocketEventReceived, SocketMode},
    sources::util::{SocketListenAddr, TcpSource},
    tls::TlsConfig,
};
use bytes::Bytes;
use codec::BytesDelimitedCodec;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

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
    type Error = std::io::Error;
    type Decoder = BytesDelimitedCodec;

    fn decoder(&self) -> Self::Decoder {
        BytesDelimitedCodec::new_with_max_length(b'\n', self.config.max_length)
    }

    fn build_event(&self, frame: Bytes, host: Bytes) -> Option<Event> {
        let byte_size = frame.len();
        let mut event = Event::from(frame);

        event.as_mut_log().insert(
            crate::config::log_schema().source_type_key(),
            Bytes::from("socket"),
        );

        let host_key = (self.config.host_key.clone())
            .unwrap_or_else(|| Atom::from(crate::config::log_schema().host_key()));

        event.as_mut_log().insert(host_key, host);

        emit!(SocketEventReceived {
            byte_size,
            mode: SocketMode::Tcp
        });

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
