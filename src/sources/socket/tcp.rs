use crate::{
    codec::{self, DecodingConfig},
    event::Event,
    internal_events::{SocketEventsReceived, SocketMode},
    sources::util::{SocketListenAddr, TcpSource},
    tcp::TcpKeepaliveConfig,
    tls::TlsConfig,
};
use bytes::Bytes;
use getset::{CopyGetters, Getters, Setters};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

#[derive(Deserialize, Serialize, Debug, Clone, Getters, CopyGetters, Setters)]
pub struct TcpConfig {
    #[get_copy = "pub"]
    address: SocketListenAddr,
    #[get_copy = "pub"]
    keepalive: Option<TcpKeepaliveConfig>,
    #[serde(default = "default_shutdown_timeout_secs")]
    #[getset(get_copy = "pub", set = "pub")]
    shutdown_timeout_secs: u64,
    #[get = "pub"]
    host_key: Option<String>,
    #[getset(get = "pub", set = "pub")]
    tls: Option<TlsConfig>,
    #[get_copy = "pub"]
    receive_buffer_bytes: Option<usize>,
    #[serde(flatten)]
    #[getset(get = "pub", set = "pub")]
    decoding: DecodingConfig,
}

fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl TcpConfig {
    pub fn new(
        address: SocketListenAddr,
        keepalive: Option<TcpKeepaliveConfig>,
        shutdown_timeout_secs: u64,
        host_key: Option<String>,
        tls: Option<TlsConfig>,
        receive_buffer_bytes: Option<usize>,
        decoding: Option<DecodingConfig>,
    ) -> Self {
        Self {
            address,
            keepalive,
            shutdown_timeout_secs,
            host_key,
            tls,
            receive_buffer_bytes,
            decoding: decoding.unwrap_or_default(),
        }
    }

    pub fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            keepalive: None,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            host_key: None,
            tls: None,
            receive_buffer_bytes: None,
            decoding: Default::default(),
        }
    }
}

#[derive(Clone)]
pub struct RawTcpSource {
    config: TcpConfig,
    decoder: codec::Decoder,
}

impl RawTcpSource {
    pub fn new(config: TcpConfig, decoder: codec::Decoder) -> Self {
        Self { config, decoder }
    }
}

impl TcpSource for RawTcpSource {
    type Error = codec::Error;
    type Item = SmallVec<[Event; 1]>;
    type Decoder = codec::Decoder;

    fn decoder(&self) -> Self::Decoder {
        self.decoder.clone()
    }

    fn handle_events(&self, events: &mut [Event], host: Bytes, byte_size: usize) {
        emit!(SocketEventsReceived {
            mode: SocketMode::Tcp,
            count: events.len(),
            byte_size,
        });

        for event in events {
            let log = event.as_mut_log();
            log.insert(
                crate::config::log_schema().source_type_key(),
                Bytes::from("socket"),
            );

            let host_key = (self.config.host_key.clone())
                .unwrap_or_else(|| crate::config::log_schema().host_key().to_string());

            log.insert(host_key, host.clone());
        }
    }
}
