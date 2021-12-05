use crate::{
    codecs::{
        self,
        decoding::{DeserializerConfig, FramingConfig},
    },
    config::log_schema,
    event::Event,
    internal_events::{SocketEventsReceived, SocketMode},
    serde::default_decoding,
    sources::util::{SocketListenAddr, TcpNullAcker, TcpSource},
    tcp::TcpKeepaliveConfig,
    tls::TlsConfig,
};
use bytes::Bytes;
use chrono::Utc;
use getset::{CopyGetters, Getters, Setters};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

#[derive(Deserialize, Serialize, Debug, Clone, Getters, CopyGetters, Setters)]
pub struct TcpConfig {
    #[get_copy = "pub"]
    address: SocketListenAddr,
    #[get_copy = "pub"]
    keepalive: Option<TcpKeepaliveConfig>,
    #[getset(get_copy = "pub", set = "pub")]
    max_length: Option<usize>,
    #[serde(default = "default_shutdown_timeout_secs")]
    #[getset(get_copy = "pub", set = "pub")]
    shutdown_timeout_secs: u64,
    #[get = "pub"]
    host_key: Option<String>,
    #[getset(get = "pub", set = "pub")]
    tls: Option<TlsConfig>,
    #[get_copy = "pub"]
    receive_buffer_bytes: Option<usize>,
    #[getset(get = "pub", set = "pub")]
    framing: Option<Box<dyn FramingConfig>>,
    #[serde(default = "default_decoding")]
    #[getset(get = "pub", set = "pub")]
    decoding: Box<dyn DeserializerConfig>,
}

const fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl TcpConfig {
    pub fn new(
        address: SocketListenAddr,
        keepalive: Option<TcpKeepaliveConfig>,
        max_length: Option<usize>,
        shutdown_timeout_secs: u64,
        host_key: Option<String>,
        tls: Option<TlsConfig>,
        receive_buffer_bytes: Option<usize>,
        framing: Option<Box<dyn FramingConfig>>,
        decoding: Box<dyn DeserializerConfig>,
    ) -> Self {
        Self {
            address,
            keepalive,
            max_length,
            shutdown_timeout_secs,
            host_key,
            tls,
            receive_buffer_bytes,
            framing,
            decoding,
        }
    }

    pub fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            keepalive: None,
            max_length: Some(crate::serde::default_max_length()),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            host_key: None,
            tls: None,
            receive_buffer_bytes: None,
            framing: None,
            decoding: default_decoding(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawTcpSource {
    config: TcpConfig,
    decoder: codecs::Decoder,
}

impl RawTcpSource {
    pub const fn new(config: TcpConfig, decoder: codecs::Decoder) -> Self {
        Self { config, decoder }
    }
}

impl TcpSource for RawTcpSource {
    type Error = codecs::decoding::Error;
    type Item = SmallVec<[Event; 1]>;
    type Decoder = codecs::Decoder;
    type Acker = TcpNullAcker;

    fn decoder(&self) -> Self::Decoder {
        self.decoder.clone()
    }

    fn handle_events(&self, events: &mut [Event], host: Bytes, byte_size: usize) {
        emit!(&SocketEventsReceived {
            mode: SocketMode::Tcp,
            byte_size,
            count: events.len()
        });

        let now = Utc::now();

        for event in events {
            if let Event::Log(ref mut log) = event {
                log.try_insert(log_schema().source_type_key(), Bytes::from("socket"));
                log.try_insert(log_schema().timestamp_key(), now);

                let host_key = (self.config.host_key.clone())
                    .unwrap_or_else(|| log_schema().host_key().to_string());

                log.try_insert(host_key, host.clone());
            }
        }
    }

    fn build_acker(&self, _: &Self::Item) -> Self::Acker {
        TcpNullAcker
    }
}
