use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use async_trait::async_trait;
use vector_config::{component::GenerateConfig, configurable_component};
use vector_core::{
    config::{AcknowledgementsConfig, Input},
    sink::VectorSink,
};

use crate::{
    config::{SinkConfig, SinkContext},
    sinks::{
        util::{
            tcp::TcpSinkConfig, udp::UdpSinkConfig, unix::UnixSinkConfig, BatchConfig,
            SinkBatchSettings,
        },
        Healthcheck,
    },
};

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
    Udp(StatsdUdpConfig),

    /// Send over a Unix domain socket (UDS).
    #[cfg(unix)]
    Unix(UnixSinkConfig),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StatsdDefaultBatchSettings;

impl SinkBatchSettings for StatsdDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1000);
    const MAX_BYTES: Option<usize> = Some(1300);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// UDP configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct StatsdUdpConfig {
    #[serde(flatten)]
    pub udp: UdpSinkConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<StatsdDefaultBatchSettings>,
}

fn default_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8125)
}

impl GenerateConfig for StatsdSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(&Self {
            default_namespace: None,
            mode: Mode::Udp(StatsdUdpConfig {
                batch: Default::default(),
                udp: UdpSinkConfig::from_address(default_address().to_string()),
            }),
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait]
impl SinkConfig for StatsdSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batcher_settings = self.batch_settings.into_batcher_settings()?;
        let request_builder = StatsdRequestBuilder::new(self.default_namespace.clone())?;

        let (service, protocol) = match &self.mode {
            Mode::Tcp(config) => config.build(Default::default(), encoder),
            Mode::Udp(config) => {
                let (service, healthcheck) = config.udp.build_service()?;
            }
            #[cfg(unix)]
            Mode::Unix(config) => config.build(Default::default(), encoder),
        };

        let sink = StatsdSink::new(service, request_builder, batcher_settings, protocol);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}
