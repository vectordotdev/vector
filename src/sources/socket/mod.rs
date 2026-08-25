pub mod tcp;
pub mod udp;
#[cfg(unix)]
mod unix;

use vector_lib::{
    codecs::decoding::DeserializerConfig,
    config::{LegacyKey, LogNamespace, log_schema},
    configurable::configurable_component,
    lookup::{lookup_v2::OptionalValuePath, owned_value_path},
};
use vrl::value::{Kind, kind::Collection};

use crate::{
    codecs::DecodingConfig,
    config::{GenerateConfig, Resource, SourceConfig, SourceContext, SourceOutput},
    sources::util::net::TcpSource,
    tls::MaybeTlsSettings,
};

/// Configuration for the `socket` source.
#[configurable_component(source("socket", "Collect logs over a socket."))]
#[derive(Clone, Debug)]
pub struct SocketConfig {
    #[serde(flatten)]
    pub mode: Mode,
}

/// Listening mode for the `socket` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The type of socket to use."))]
#[allow(clippy::large_enum_variant)] // just used for configuration
pub enum Mode {
    /// Listen on TCP.
    Tcp(tcp::TcpConfig),

    /// Listen on UDP.
    Udp(udp::UdpConfig),

    /// Listen on a Unix domain socket (UDS), in datagram mode.
    #[cfg(unix)]
    UnixDatagram(unix::UnixConfig),

    /// Listen on a Unix domain socket (UDS), in stream mode.
    #[cfg(unix)]
    #[serde(alias = "unix")]
    UnixStream(unix::UnixConfig),
}

impl SocketConfig {
    pub fn new_tcp(tcp_config: tcp::TcpConfig) -> Self {
        tcp_config.into()
    }

    pub fn make_basic_tcp_config(addr: std::net::SocketAddr) -> Self {
        tcp::TcpConfig::from_address(addr.into()).into()
    }

    fn decoding(&self) -> DeserializerConfig {
        match &self.mode {
            Mode::Tcp(config) => config.decoding().clone(),
            Mode::Udp(config) => config.decoding().clone(),
            #[cfg(unix)]
            Mode::UnixDatagram(config) => config.decoding().clone(),
            #[cfg(unix)]
            Mode::UnixStream(config) => config.decoding().clone(),
        }
    }

    fn log_namespace(&self, global_log_namespace: LogNamespace) -> LogNamespace {
        match &self.mode {
            Mode::Tcp(config) => global_log_namespace.merge(config.log_namespace),
            Mode::Udp(config) => global_log_namespace.merge(config.log_namespace),
            #[cfg(unix)]
            Mode::UnixDatagram(config) => global_log_namespace.merge(config.log_namespace),
            #[cfg(unix)]
            Mode::UnixStream(config) => global_log_namespace.merge(config.log_namespace),
        }
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

impl GenerateConfig for SocketConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r#"mode: tcp
            address: "0.0.0.0:9000""#,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "socket")]
impl SourceConfig for SocketConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        match self.mode.clone() {
            Mode::Tcp(config) => {
                let log_namespace = cx.log_namespace(config.log_namespace);

                let decoding = config.decoding().clone();
                let decoder = DecodingConfig::new(
                    config
                        .framing
                        .clone()
                        .unwrap_or_else(|| decoding.default_stream_framing()),
                    decoding,
                    log_namespace,
                )
                .build()?;

                let tcp = tcp::RawTcpSource::new(config.clone(), decoder, log_namespace);
                let tls_config = config.tls().as_ref().map(|tls| tls.tls_config.clone());
                let tls_client_metadata_key = config
                    .tls()
                    .as_ref()
                    .and_then(|tls| tls.client_metadata_key.clone())
                    .and_then(|k| k.path);
                let tls = MaybeTlsSettings::from_config(tls_config.as_ref(), true)?;
                tcp.run(
                    config.address(),
                    config.keepalive(),
                    config.shutdown_timeout_secs(),
                    tls,
                    None, // tls_reloader: not wired for this source
                    tls_client_metadata_key,
                    config.receive_buffer_bytes(),
                    config.max_connection_duration_secs(),
                    config.tls_handshake_timeout_secs(),
                    cx,
                    false.into(),
                    config.connection_limit,
                    config.permit_origin.map(Into::into),
                    SocketConfig::NAME,
                    log_namespace,
                )
            }
            Mode::Udp(config) => {
                let log_namespace = cx.log_namespace(config.log_namespace);
                let decoding = config.decoding().clone();
                let framing = config
                    .framing()
                    .clone()
                    .unwrap_or_else(|| decoding.default_message_based_framing());
                let decoder = DecodingConfig::new(framing, decoding, log_namespace).build()?;
                Ok(udp::udp(
                    config,
                    decoder,
                    cx.shutdown,
                    cx.out,
                    log_namespace,
                ))
            }
            #[cfg(unix)]
            Mode::UnixDatagram(config) => {
                let log_namespace = cx.log_namespace(config.log_namespace);
                let decoding = config.decoding.clone();
                let framing = config
                    .framing
                    .clone()
                    .unwrap_or_else(|| decoding.default_message_based_framing());
                let decoder = DecodingConfig::new(framing, decoding, log_namespace).build()?;

                unix::unix_datagram(config, decoder, cx.shutdown, cx.out, log_namespace)
            }
            #[cfg(unix)]
            Mode::UnixStream(config) => {
                let log_namespace = cx.log_namespace(config.log_namespace);

                let decoding = config.decoding().clone();
                let decoder = DecodingConfig::new(
                    config
                        .framing
                        .clone()
                        .unwrap_or_else(|| decoding.default_stream_framing()),
                    decoding,
                    log_namespace,
                )
                .build()?;

                unix::unix_stream(config, decoder, cx.shutdown, cx.out, log_namespace)
            }
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = self.log_namespace(global_log_namespace);

        let schema_definition = self
            .decoding()
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        let schema_definition = match &self.mode {
            Mode::Tcp(config) => {
                let legacy_host_key = config.host_key().path.map(LegacyKey::InsertIfEmpty);

                let legacy_port_key = config.port_key().clone().path.map(LegacyKey::InsertIfEmpty);

                let tls_client_metadata_path = config
                    .tls()
                    .as_ref()
                    .and_then(|tls| tls.client_metadata_key.as_ref())
                    .and_then(|k| k.path.clone())
                    .map(LegacyKey::Overwrite);

                schema_definition
                    .with_source_metadata(
                        Self::NAME,
                        legacy_host_key,
                        &owned_value_path!("host"),
                        Kind::bytes(),
                        Some("host"),
                    )
                    .with_source_metadata(
                        Self::NAME,
                        legacy_port_key,
                        &owned_value_path!("port"),
                        Kind::integer(),
                        None,
                    )
                    .with_source_metadata(
                        Self::NAME,
                        tls_client_metadata_path,
                        &owned_value_path!("tls_client_metadata"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                            .or_undefined(),
                        None,
                    )
            }
            Mode::Udp(config) => {
                let legacy_host_key = config.host_key().path.map(LegacyKey::InsertIfEmpty);

                let legacy_port_key = config.port_key().clone().path.map(LegacyKey::InsertIfEmpty);

                schema_definition
                    .with_source_metadata(
                        Self::NAME,
                        legacy_host_key,
                        &owned_value_path!("host"),
                        Kind::bytes(),
                        None,
                    )
                    .with_source_metadata(
                        Self::NAME,
                        legacy_port_key,
                        &owned_value_path!("port"),
                        Kind::integer(),
                        None,
                    )
            }
            #[cfg(unix)]
            Mode::UnixDatagram(config) => {
                let legacy_host_key = config.host_key().clone().path.map(LegacyKey::InsertIfEmpty);

                schema_definition.with_source_metadata(
                    Self::NAME,
                    legacy_host_key,
                    &owned_value_path!("host"),
                    Kind::bytes(),
                    None,
                )
            }
            #[cfg(unix)]
            Mode::UnixStream(config) => {
                let legacy_host_key = config.host_key().clone().path.map(LegacyKey::InsertIfEmpty);

                schema_definition.with_source_metadata(
                    Self::NAME,
                    legacy_host_key,
                    &owned_value_path!("host"),
                    Kind::bytes(),
                    None,
                )
            }
        };

        vec![SourceOutput::new_maybe_logs(
            self.decoding().output_type(),
            schema_definition,
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        match self.mode.clone() {
            Mode::Tcp(tcp) => vec![tcp.address().as_tcp_resource()],
            Mode::Udp(udp) => vec![udp.address().as_udp_resource()],
            #[cfg(unix)]
            Mode::UnixDatagram(_) => vec![],
            #[cfg(unix)]
            Mode::UnixStream(_) => vec![],
        }
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

pub(crate) fn default_host_key() -> OptionalValuePath {
    log_schema().host_key().cloned().into()
}

#[cfg(test)]
mod test;
