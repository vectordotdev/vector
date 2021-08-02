use crate::{
    event::Event,
    internal_events::{SocketEventReceived, SocketMode},
    sources::util::{decoding::DecodingConfig, SocketListenAddr, TcpSource},
    tcp::TcpKeepaliveConfig,
    tls::TlsConfig,
};
use bytes::Bytes;
use getset::{CopyGetters, Getters, Setters};
use serde::{Deserialize, Serialize};

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
    #[getset(get_copy = "pub", set = "pub")]
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

pub struct RawTcpSource<D: tokio_util::codec::Decoder<Item = (Event, usize)>> {
    config: TcpConfig,
    build_decoder: Box<dyn Fn() -> D + Send + Sync>,
}

impl<D: tokio_util::codec::Decoder<Item = (Event, usize)>> RawTcpSource<D> {
    pub fn new(config: TcpConfig, build_decoder: Box<dyn Fn() -> D + Send + Sync>) -> Self {
        Self {
            config,
            build_decoder,
        }
    }
}

impl<D> TcpSource for RawTcpSource<D>
where
    D: tokio_util::codec::Decoder<Item = (Event, usize)> + Send + Sync + 'static,
    D::Error: From<std::io::Error>
        + crate::sources::util::TcpIsErrorFatal
        + std::fmt::Debug
        + std::fmt::Display
        + Send,
{
    type Error = D::Error;
    type Item = Event;
    type Decoder = D;

    fn build_decoder(&self) -> Self::Decoder {
        (self.build_decoder)()
    }

    fn handle_event(&self, event: &mut Event, host: Bytes, byte_size: usize) {
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
    }
}
