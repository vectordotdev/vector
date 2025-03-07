use futures::FutureExt;
use http::Request;
use hyper::Body;
use std::str::FromStr;
use vector_lib::{
    codecs::JsonSerializerConfig, configurable::configurable_component, tls::TlsEnableableConfig,
};

use crate::{
    codecs::Transformer,
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        util::{
            service::TowerRequestConfigDefaults, BatchConfig, Compression, SinkBatchSettings,
            TowerRequestConfig, UriSerde,
        },
        HealthcheckError,
    },
};

use super::service::OpentelemetryMetricsSvc;

#[derive(Clone, Copy, Debug, Default)]
pub struct OpentelemetryMetricsDefaultBatchSettings;

impl SinkBatchSettings for OpentelemetryMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Clone, Copy, Debug)]
pub struct OpentelemetryMetricsTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for OpentelemetryMetricsTowerRequestConfigDefaults {
    const RATE_LIMIT_NUM: u64 = 150;
}

/// Configuration for the `opentelemetry_metrics` sink.
#[configurable_component(sink(
    "opentelemetry_metrics",
    "Publish metric events to an OpenTelemetry collector."
))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryMetricsSinkConfig {
    /// The endpoint to send OpenTelemetry metrics to.
    ///
    /// This should be a full URL, including the protocol (e.g. `https://`).
    #[configurable(metadata(docs::examples = "http://localhost:4317"))]
    pub endpoint: String,

    /// The endpoint to send healthcheck requests to.
    ///
    /// This should be a full URL, including the protocol (e.g. `https://`).
    #[configurable(metadata(docs::examples = "http://localhost:13133"))]
    pub healthcheck_endpoint: String,

    /// The default namespace to use for metrics that do not have one.
    ///
    /// Metrics with the same name can only be differentiated by their namespace.
    #[configurable(metadata(docs::examples = "myservice"))]
    pub default_namespace: Option<String>,

    /// The aggregation temporality to use for metrics.
    ///
    /// This determines how metrics are aggregated over time.
    #[serde(default)]
    pub aggregation_temporality: AggregationTemporalityConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<OpentelemetryMetricsDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub encoding: OpentelemetryMetricsEncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

/// The aggregation temporality to use for metrics.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AggregationTemporalityConfig {
    /// Delta temporality means that metrics are reported as changes since the last report.
    Delta,
    /// Cumulative temporality means that metrics are reported as cumulative changes since a fixed start time.
    Cumulative,
}

impl Default for AggregationTemporalityConfig {
    fn default() -> Self {
        Self::Cumulative
    }
}

/// Encoding configuration for OpenTelemetry Metrics.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[configurable(description = "Configures how events are encoded into raw bytes.")]
pub struct OpentelemetryMetricsEncodingConfig {
    #[serde(flatten)]
    encoding: JsonSerializerConfig,

    #[serde(flatten)]
    transformer: Transformer,
}

impl_generate_config_from_default!(OpentelemetryMetricsSinkConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "opentelemetry_metrics")]
impl SinkConfig for OpentelemetryMetricsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = HttpClient::new(None, cx.proxy())?;
        let uri = UriSerde::from_str(&self.healthcheck_endpoint)
            .map_err(|e| crate::Error::from(format!("Invalid healthcheck endpoint: {}", e)))?;

        let healthcheck = healthcheck(uri.to_string(), client.clone()).boxed();
        let sink = OpentelemetryMetricsSvc::new(self.clone(), client)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

/// Healthcheck for the `opentelemetry_metrics` sink.
///
/// Reference https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/main/extension/healthcheckextension/README.md
async fn healthcheck(endpoint: String, client: HttpClient) -> crate::Result<()> {
    let request = Request::head(&endpoint)
        .body(Body::empty())
        .map_err(|e| crate::Error::from(format!("Error building request: {}", e)))?;

    let response = client.send(request).await?;

    match response.status() {
        http::StatusCode::OK | http::StatusCode::ACCEPTED | http::StatusCode::NO_CONTENT => Ok(()),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use wiremock::{
        matchers::method,
        Mock, MockServer, ResponseTemplate,
    };

    #[test]
    fn generate_config() {
        let config = indoc! {r#"
            endpoint = "http://localhost:4317"
            healthcheck_endpoint = "http://localhost:13133"
            default_namespace = "vector"
            aggregation_temporality = "delta"
        "#};
        let config: OpentelemetryMetricsSinkConfig = toml::from_str(config).unwrap();

        assert_eq!(config.endpoint, "http://localhost:4317");
        assert_eq!(config.healthcheck_endpoint, "http://localhost:13133");
        assert_eq!(config.default_namespace, Some("vector".to_string()));
        assert!(matches!(
            config.aggregation_temporality,
            AggregationTemporalityConfig::Delta
        ));
    }

    #[tokio::test]
    async fn healthcheck_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(None, &Default::default()).unwrap();
        let endpoint = mock_server.uri();

        healthcheck(endpoint, client).await.unwrap();
    }

    #[tokio::test]
    async fn healthcheck_failure() {
        let mock_server = MockServer::start().await;

        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(None, &Default::default()).unwrap();
        let endpoint = mock_server.uri();

        assert!(healthcheck(endpoint, client).await.is_err());
    }
}
