use std::time::Duration;

use chrono::Utc;
use serde_with::serde_as;
use smallvec::SmallVec;
use vector_lib::codecs::decoding::{DeserializerConfig, FramingConfig};
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path, path};

use crate::{
    codecs::Decoder,
    event::Event,
    serde::default_decoding,
    sources::util::net::{SocketListenAddr, TcpNullAcker, TcpSource},
    tcp::TcpKeepaliveConfig,
    tls::TlsSourceConfig,
};

use super::{default_host_key, SocketConfig};

/// TCP configuration for the `socket` source.
#[serde_as]
#[configurable_component]
#[derive(Clone, Debug)]
pub struct TcpConfig {
    #[configurable(derived)]
    address: SocketListenAddr,

    #[configurable(derived)]
    keepalive: Option<TcpKeepaliveConfig>,

    /// The timeout before a connection is forcefully closed during shutdown.
    #[serde(default = "default_shutdown_timeout_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Shutdown Timeout"))]
    shutdown_timeout_secs: Duration,

    /// Overrides the name of the log field used to add the peer host to each event.
    ///
    /// The value will be the peer host's address, including the port i.e. `1.2.3.4:9000`.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// Set to `""` to suppress this key.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    #[serde(default = "default_host_key")]
    host_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the peer host's port to each event.
    ///
    /// The value will be the peer host's port i.e. `9000`.
    ///
    /// By default, `"port"` is used.
    ///
    /// Set to `""` to suppress this key.
    #[serde(default = "default_port_key")]
    port_key: OptionalValuePath,

    #[configurable(derived)]
    tls: Option<TlsSourceConfig>,

    /// The size of the receive buffer used for each connection.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    receive_buffer_bytes: Option<usize>,

    /// Maximum duration to keep each connection open. Connections open for longer than this duration are closed.
    ///
    /// This is helpful for load balancing long-lived connections.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    max_connection_duration_secs: Option<u64>,

    /// The maximum number of TCP connections that are allowed at any given time.
    #[configurable(metadata(docs::type_unit = "connections"))]
    pub connection_limit: Option<u32>,

    #[configurable(derived)]
    pub(super) framing: Option<FramingConfig>,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub(super) decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    pub log_namespace: Option<bool>,
}

const fn default_shutdown_timeout_secs() -> Duration {
    Duration::from_secs(30)
}

fn default_port_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("port"))
}

impl TcpConfig {
    pub fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            keepalive: None,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            host_key: default_host_key(),
            port_key: default_port_key(),
            tls: None,
            receive_buffer_bytes: None,
            max_connection_duration_secs: None,
            framing: None,
            decoding: default_decoding(),
            connection_limit: None,
            log_namespace: None,
        }
    }

    pub const fn host_key(&self) -> &OptionalValuePath {
        &self.host_key
    }

    pub const fn port_key(&self) -> &OptionalValuePath {
        &self.port_key
    }

    pub const fn tls(&self) -> &Option<TlsSourceConfig> {
        &self.tls
    }

    pub const fn framing(&self) -> &Option<FramingConfig> {
        &self.framing
    }

    pub const fn decoding(&self) -> &DeserializerConfig {
        &self.decoding
    }

    pub const fn address(&self) -> SocketListenAddr {
        self.address
    }

    pub const fn keepalive(&self) -> Option<TcpKeepaliveConfig> {
        self.keepalive
    }

    pub const fn shutdown_timeout_secs(&self) -> Duration {
        self.shutdown_timeout_secs
    }

    pub const fn receive_buffer_bytes(&self) -> Option<usize> {
        self.receive_buffer_bytes
    }

    pub const fn max_connection_duration_secs(&self) -> Option<u64> {
        self.max_connection_duration_secs
    }

    pub fn set_max_connection_duration_secs(&mut self, val: Option<u64>) -> &mut Self {
        self.max_connection_duration_secs = val;
        self
    }

    pub fn set_shutdown_timeout_secs(&mut self, val: u64) -> &mut Self {
        self.shutdown_timeout_secs = Duration::from_secs(val);
        self
    }

    pub fn set_tls(&mut self, val: Option<TlsSourceConfig>) -> &mut Self {
        self.tls = val;
        self
    }

    pub fn set_framing(&mut self, val: Option<FramingConfig>) -> &mut Self {
        self.framing = val;
        self
    }

    pub fn set_decoding(&mut self, val: DeserializerConfig) -> &mut Self {
        self.decoding = val;
        self
    }

    pub fn set_log_namespace(&mut self, val: Option<bool>) -> &mut Self {
        self.log_namespace = val;
        self
    }
}

#[derive(Clone)]
pub struct RawTcpSource {
    config: TcpConfig,
    decoder: Decoder,
    log_namespace: LogNamespace,
}

impl RawTcpSource {
    pub const fn new(config: TcpConfig, decoder: Decoder, log_namespace: LogNamespace) -> Self {
        Self {
            config,
            decoder,
            log_namespace,
        }
    }
}

impl TcpSource for RawTcpSource {
    type Error = vector_lib::codecs::decoding::Error;
    type Item = SmallVec<[Event; 1]>;
    type Decoder = Decoder;
    type Acker = TcpNullAcker;

    fn decoder(&self) -> Self::Decoder {
        self.decoder.clone()
    }

    fn handle_events(&self, events: &mut [Event], host: std::net::SocketAddr) {
        let now = Utc::now();

        for event in events {
            if let Event::Log(ref mut log) = event {
                self.log_namespace.insert_standard_vector_source_metadata(
                    log,
                    SocketConfig::NAME,
                    now,
                );

                let legacy_host_key = self.config.host_key.clone().path;

                self.log_namespace.insert_source_metadata(
                    SocketConfig::NAME,
                    log,
                    legacy_host_key.as_ref().map(LegacyKey::InsertIfEmpty),
                    path!("host"),
                    host.ip().to_string(),
                );

                let legacy_port_key = self.config.port_key.clone().path;

                self.log_namespace.insert_source_metadata(
                    SocketConfig::NAME,
                    log,
                    legacy_port_key.as_ref().map(LegacyKey::InsertIfEmpty),
                    path!("port"),
                    host.port(),
                );
            }
        }
    }

    fn build_acker(&self, _: &[Self::Item]) -> Self::Acker {
        TcpNullAcker
    }
}
