use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use async_trait::async_trait;
use vector_common::internal_event::Protocol;
use vector_config::{component::GenerateConfig, configurable_component};
use vector_core::{
    config::{AcknowledgementsConfig, Input},
    sink::VectorSink,
};

use crate::{
    config::{SinkConfig, SinkContext},
    internal_events::SocketMode,
    sinks::{
        util::{
            service::udp::{UdpConnector, UdpConnectorConfig},
            tcp::TcpSinkConfig,
            unix::UnixSinkConfig,
            BatchConfig, SinkBatchSettings,
        },
        Healthcheck,
    },
};

use super::{request_builder::StatsdRequestBuilder, sink::StatsdSink};

#[derive(Clone, Copy, Debug, Default)]
pub struct StatsdDefaultBatchSettings;

impl SinkBatchSettings for StatsdDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1000);
    const MAX_BYTES: Option<usize> = Some(1300);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `statsd` sink.
#[configurable_component(sink("statsd"))]
#[derive(Clone, Debug)]
pub struct StatsdSinkConfig {
    /// Sets the default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with a period (`.`).
    #[serde(alias = "namespace")]
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
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
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
    Tcp(TcpSinkConfig),

    /// Send over UDP.
    Udp(UdpConnectorConfig),

    /// Send over a Unix domain socket (UDS).
    #[cfg(unix)]
    Unix(UnixSinkConfig),
}

fn default_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8125)
}

impl GenerateConfig for StatsdSinkConfig {
    fn generate_config() -> toml::Value {
        let address = default_address();

        toml::Value::try_from(&Self {
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
impl SinkConfig for StatsdSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batcher_settings = self.batch.into_batcher_settings()?;

        let (service, healthcheck, socket_mode) = match &self.mode {
            Mode::Tcp(config) => config.build(Default::default(), encoder),
            Mode::Udp(config) => {
                let connector = UdpConnector::from(config.clone());
                let service = connector.service();
                let healthcheck = connector.healthcheck();

                (service, healthcheck, SocketMode::Udp)
            }
            #[cfg(unix)]
            Mode::Unix(config) => config.build(Default::default(), encoder),
        };

        let request_builder =
            StatsdRequestBuilder::new(self.default_namespace.clone(), socket_mode)?;
        let protocol = Protocol::from(socket_mode.as_str());
        let sink = StatsdSink::new(service, batcher_settings, request_builder, protocol);

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
