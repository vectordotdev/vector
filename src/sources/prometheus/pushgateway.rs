use std::net::SocketAddr;
use vector_config::configurable_component;

use vector_core::config::LogNamespace;

use crate::{
    config::{
        GenerateConfig, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    event::Event,
    internal_events::PrometheusRemoteWriteParseError,
    serde::bool_or_struct,
    sources::{
        self,
        util::{decode, http::HttpMethod, ErrorMessage, HttpSource, HttpSourceAuthConfig},
    },
    tls::TlsEnableableConfig,
};


#[configurable_component(source(
    "prometheus_pushgateway",
    "Receive metrics via the Prometheus Pushgateway protocol."
))]
#[derive(Clone, Debug)]
pub struct PrometheusPushgatewayConfig {
    /// The socket address to accept connections on.
    ///
    /// The address _must_ include a port.
    #[configurable(metadata(docs::examples = "0.0.0.0:9091"))]
    address: SocketAddr,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    auth: Option<HttpSourceAuthConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,
}

impl GenerateConfig for PrometheusPushgatewayConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "127.0.0.1:9091".parse().unwrap(),
            tls: None,
            auth: None,
            acknowledgements: SourceAcknowledgementsConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_remote_write")]
impl SourceConfig for PrometheusPushgatewayConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let source = PushgatewaySource;
        source.run(
            self.address,
            // TODO: Support configuring path so we can run multiple of these
            "",
            HttpMethod::Post,
            // TODO: We probably want non-strict paths so we can parse grouping keys
            true,
            &self.tls,
            &self.auth,
            cx,
            self.acknowledgements,
        )
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[derive(Clone)]
struct PushgatewaySource;
