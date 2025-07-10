use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use async_trait::async_trait;
use vector_lib::configurable::{component::GenerateConfig, configurable_component};
use vector_lib::internal_event::Protocol;
use vector_lib::{
    config::{AcknowledgementsConfig, Input},
    sink::VectorSink,
};

use crate::{
    config::{SinkConfig, SinkContext},
    internal_events::SocketMode,
    sinks::{
        util::{
            service::net::{NetworkConnector, TcpConnectorConfig, UdpConnectorConfig},
            BatchConfig, SinkBatchSettings,
        },
        Healthcheck,
    },
};

#[cfg(unix)]
use crate::sinks::util::service::net::UnixConnectorConfig;

use super::{request_builder::StatsdRequestBuilder, service::StatsdService, sink::StatsdSink};

#[derive(Clone, Copy, Debug, Default)]
pub struct StatsdDefaultBatchSettings;

impl SinkBatchSettings for StatsdDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1000);
    const MAX_BYTES: Option<usize> = Some(1300);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `statsd` sink.
#[configurable_component(sink("statsd", "Deliver metric data to a StatsD aggregator."))]
#[derive(Clone, Debug)]
pub struct StatsdSinkConfig {
    /// Sets the default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with a period (`.`).
    #[serde(alias = "namespace")]
    #[configurable(metadata(docs::examples = "service"))]
    pub default_namespace: Option<String>,

    #[serde(flatten)]
    pub mode: Mode,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<StatsdDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

/// Socket mode.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The type of socket to use."))]
pub enum Mode {
    /// Send over TCP.
    Tcp(TcpConnectorConfig),

    /// Send over UDP.
    Udp(UdpConnectorConfig),

    /// Send over a Unix domain socket (UDS).
    #[cfg(unix)]
    Unix(UnixConnectorConfig),
}

impl Mode {
    const fn as_socket_mode(&self) -> SocketMode {
        match self {
            Self::Tcp(_) => SocketMode::Tcp,
            Self::Udp(_) => SocketMode::Udp,
            #[cfg(unix)]
            Self::Unix(_) => SocketMode::Unix,
        }
    }

    fn as_connector(&self) -> NetworkConnector {
        match self {
            Self::Tcp(config) => config.as_connector(),
            Self::Udp(config) => config.as_connector(),
            #[cfg(unix)]
            Self::Unix(config) => config.as_connector(),
        }
    }
}

const fn default_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8125)
}

impl GenerateConfig for StatsdSinkConfig {
    fn generate_config() -> toml::Value {
        let address = default_address();

        toml::Value::try_from(Self {
            default_namespace: None,
            mode: Mode::Udp(UdpConnectorConfig::from_address(
                address.ip().to_string(),
                address.port(),
            )),
            batch: Default::default(),
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait]
#[typetag::serde(name = "statsd")]
impl SinkConfig for StatsdSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batcher_settings = self.batch.into_batcher_settings()?;

        let socket_mode = self.mode.as_socket_mode();
        let request_builder =
            StatsdRequestBuilder::new(self.default_namespace.clone(), socket_mode);
        let protocol = Protocol::from(socket_mode.as_str());

        let connector = self.mode.as_connector();
        let service = connector.service();
        let healthcheck = connector.healthcheck();

        let sink = StatsdSink::new(
            StatsdService::from_transport(service),
            batcher_settings,
            request_builder,
            protocol,
        );
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[cfg(test)]
mod test {
    use super::StatsdSinkConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StatsdSinkConfig>();
    }
}
