use crate::{
    event::Event,
    internal_events::{SocketEventReceived, SocketMode},
    sources::util::{SocketListenAddr, TcpSource},
    tcp::TcpKeepaliveConfig,
    tls::TlsConfig,
};
use bytes::Bytes;
use codec::BytesDelimitedCodec;
use getset::{CopyGetters, Getters, Setters};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone, Getters, CopyGetters, Setters)]
pub struct TcpConfig {
    #[get_copy = "pub"]
    address: SocketListenAddr,
    #[get_copy = "pub"]
    keepalive: Option<TcpKeepaliveConfig>,
    #[serde(default = "default_max_length")]
    #[getset(get_copy = "pub", set = "pub")]
    max_length: usize,
    #[serde(default = "default_shutdown_timeout_secs")]
    #[getset(get_copy = "pub", set = "pub")]
    shutdown_timeout_secs: u64,
    #[get = "pub"]
    host_key: Option<String>,
    #[getset(get = "pub", set = "pub")]
    tls: Option<TlsConfig>,
    #[get_copy = "pub"]
    receive_buffer_bytes: Option<usize>,
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl TcpConfig {
    pub fn new(
        address: SocketListenAddr,
        keepalive: Option<TcpKeepaliveConfig>,
        max_length: usize,
        shutdown_timeout_secs: u64,
        host_key: Option<String>,
        tls: Option<TlsConfig>,
        receive_buffer_bytes: Option<usize>,
    ) -> Self {
        Self {
            address,
            keepalive,
            max_length,
            shutdown_timeout_secs,
            host_key,
            tls,
            receive_buffer_bytes,
        }
    }

    pub fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            keepalive: None,
            max_length: default_max_length(),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            host_key: None,
            tls: None,
            receive_buffer_bytes: None,
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
            .unwrap_or_else(|| crate::config::log_schema().host_key().to_string());

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
