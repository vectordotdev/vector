use std::{collections::HashMap, net::SocketAddr};

use bytes::Bytes;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;
use vector_core::event::{Metric, MetricKind, MetricValue};
use warp::http::HeaderMap;

use crate::{
    config::{
        GenerateConfig, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    event::Event,
    serde::bool_or_struct,
    sources::{
        self,
        util::{http::HttpMethod, ErrorMessage, HttpSource, HttpSourceAuthConfig},
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

impl PushgatewaySource {
    fn decode_body(&self, body: Bytes) -> Result<Vec<Event>, ErrorMessage> {
        let mut result = Vec::new();

        let counter = Metric::new(
            "foo",
            MetricKind::Absolute,
            MetricValue::Counter {
                value: 49.0,
            },
        );

        result.push(counter.into());

        Ok(result)
    }
}

impl HttpSource for PushgatewaySource {
    fn build_events(
        &self,
        mut body: Bytes,
        header_map: &HeaderMap,
        _query_parameters: &HashMap<String, String>,
        _full_path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        let events = self.decode_body(body)?;
        Ok(events)
    }
}
