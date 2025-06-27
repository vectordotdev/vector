use tracing::debug;

use crate::{
    codecs::{EncodingConfigWithFraming, Transformer},
    config::{AcknowledgementsConfig, SinkConfig, SinkContext},
    sinks::{http::config::HttpSinkConfig, Healthcheck, VectorSink},
};
use indoc::indoc;
use vector_config::component::GenerateConfig;
use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::configurable::configurable_component;
use vector_lib::{
    codecs::encoding::{FramingConfig, SerializerConfig},
    config::Input,
};

mod metrics;
use metrics::AggregationTemporality;

use super::http::config::HttpMethod;

/// Configuration for the `OpenTelemetry` sink.
#[configurable_component(sink(
    "opentelemetry",
    "Deliver OTLP data (logs, metrics, traces) over HTTP."
))]
#[derive(Clone, Debug)]
pub struct OpenTelemetryConfig {
    /// Protocol configuration.
    #[configurable(derived)]
    protocol: Protocol,

    /// The endpoint to send OpenTelemetry logs, metrics, and traces to.
    ///
    /// This should be a full URL, including the protocol (e.g. `https://`).
    /// If not specified, telemetry will not be sent.
    #[configurable(metadata(docs::examples = "http://localhost:4317/v1/metrics"))]
    pub endpoint: Option<String>,

    /// The endpoint to send healthcheck requests to.
    ///
    /// This should be a full URL, including the protocol (e.g. `https://`).
    #[configurable(metadata(docs::examples = "http://localhost:13133"))]
    pub healthcheck_endpoint: Option<String>,

    /// The default namespace to use for metrics that do not have one.
    ///
    /// Metrics with the same name can only be differentiated by their namespace.
    #[configurable(metadata(docs::examples = "myservice"))]
    pub default_namespace: Option<String>,

    /// The aggregation temporality to use for metrics.
    ///
    /// This determines how metrics are aggregated over time.
    #[serde(default)]
    pub aggregation_temporality: AggregationTemporality,
}

impl Default for OpenTelemetryConfig {
    fn default() -> Self {
        Self {
            protocol: Protocol::default(),
            endpoint: None,
            healthcheck_endpoint: None,
            default_namespace: Some("vector".to_string()),
            aggregation_temporality: AggregationTemporality::Cumulative,
        }
    }
}

/// The protocol used to send data to OpenTelemetry.
/// Currently only HTTP is supported, but we plan to support gRPC.
/// The proto definitions are defined [here](https://github.com/vectordotdev/vector/blob/master/lib/opentelemetry-proto/src/proto/opentelemetry-proto/opentelemetry/proto/README.md).
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case", tag = "type")]
#[configurable(metadata(docs::enum_tag_description = "The communication protocol."))]
pub enum Protocol {
    /// Send data over HTTP.
    Http(HttpSinkConfig),
}

impl Default for Protocol {
    fn default() -> Self {
        Protocol::Http(HttpSinkConfig {
            encoding: EncodingConfigWithFraming::new(
                Some(FramingConfig::NewlineDelimited),
                SerializerConfig::Json(JsonSerializerConfig::default()),
                Transformer::default(),
            ),
            uri: Default::default(),
            method: HttpMethod::Post,
            auth: Default::default(),
            headers: Default::default(),
            compression: Default::default(),
            payload_prefix: Default::default(),
            payload_suffix: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            tls: Default::default(),
            acknowledgements: Default::default(),
        })
    }
}

impl GenerateConfig for OpenTelemetryConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            [protocol]
            type = "http"
            uri = "http://localhost:5318/v1/logs"
            encoding.codec = "json"

            metrics_endpoint = "http://localhost:4317/v1/metrics"
            healthcheck_endpoint = "http://localhost:13133"
            default_namespace = "vector"
            aggregation_temporality = "cumulative"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "opentelemetry")]
impl SinkConfig for OpenTelemetryConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        use crate::sinks::util::UriSerde;
        use futures::FutureExt;
        use std::str::FromStr;
        // use vector_lib::sink::VectorSink;

        // Build the logs/traces sink
        let (logs_sink, logs_healthcheck) = match &self.protocol {
            Protocol::Http(config) => config.build(cx.clone()).await?,
        };

        // Build the metrics sink if an endpoint is provided
        let metrics_sink = if let Some(endpoint) = &self.endpoint {
            use crate::sinks::opentelemetry::metrics::OpentelemetryMetricsSvc;
            debug!(
                "Creating OpenTelemetry metrics sink with endpoint: {}",
                endpoint
            );
            let namespace = self
                .default_namespace
                .clone()
                .unwrap_or_else(|| "vector".to_string());

            let metrics_service = OpentelemetryMetricsSvc::new(
                namespace,
                endpoint.clone(),
                self.aggregation_temporality,
            )?;
            let metrics_sink = VectorSink::from_event_streamsink(metrics_service);
            Some(metrics_sink)
        } else {
            None
        };

        // Determine the healthcheck endpoint
        let healthcheck_endpoint = self.healthcheck_endpoint.clone().or_else(|| {
            let Protocol::Http(config) = &self.protocol;
            Some(config.uri.to_string())
        });

        // Create a healthcheck if an endpoint is provided
        let healthcheck = if let Some(endpoint) = healthcheck_endpoint {
            let client = crate::http::HttpClient::new(None, cx.proxy())?;
            let uri = UriSerde::from_str(&endpoint)
                .map_err(|e| crate::Error::from(format!("Invalid healthcheck endpoint: {}", e)))?;

            healthcheck(uri.to_string(), client).boxed()
        } else {
            logs_healthcheck
        };

        // Create the final sink
        let sink = if let Some(metrics_sink) = metrics_sink {
            metrics_sink
        } else {
            logs_sink
        };

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        match &self.protocol {
            Protocol::Http(config) => config.input(),
        }
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        match &self.protocol {
            Protocol::Http(config) => config.acknowledgements(),
        }
    }
}

/// Healthcheck for the OpenTelemetry sink.
///
/// Reference https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/main/extension/healthcheckextension/README.md
async fn healthcheck(endpoint: String, client: crate::http::HttpClient) -> crate::Result<()> {
    use crate::sinks::HealthcheckError;
    use http::{Request, StatusCode};
    use hyper::Body;

    let request = Request::head(&endpoint)
        .body(Body::empty())
        .map_err(|e| crate::Error::from(format!("Error building request: {}", e)))?;

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK | StatusCode::ACCEPTED | StatusCode::NO_CONTENT => Ok(()),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use indoc::indoc;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::OpenTelemetryConfig>();
    }

    #[test]
    fn config_opentelemetry_default() {
        let config = indoc! {r#"
            [protocol]
            type = "http"
            uri = "http://localhost:5318/v1/logs"
            encoding.codec = "json"
        "#};
        let config: OpenTelemetryConfig = toml::from_str(config).unwrap();

        assert!(config.endpoint.is_none());
        assert!(config.healthcheck_endpoint.is_none());
        assert_eq!(config.default_namespace, Some("vector".to_string()));
        assert!(matches!(
            config.aggregation_temporality,
            AggregationTemporality::Cumulative
        ));
    }

    #[test]
    fn config_opentelemetry_with_metrics() {
        let config = indoc! {r#"
            [protocol]
            type = "http"
            uri = "http://localhost:5318/v1/logs"
            encoding.codec = "json"

            endpoint = "http://localhost:4317/v1/metrics"
            healthcheck_endpoint = "http://localhost:13133"
            default_namespace = "myservice"
            aggregation_temporality = "delta"
        "#};
        let config: OpenTelemetryConfig = toml::from_str(config).unwrap();

        assert_eq!(
            config.endpoint,
            Some("http://localhost:4317/v1/metrics".to_string())
        );
        assert_eq!(
            config.healthcheck_endpoint,
            Some("http://localhost:13133".to_string())
        );
        assert_eq!(config.default_namespace, Some("myservice".to_string()));
        assert!(matches!(
            config.aggregation_temporality,
            AggregationTemporality::Delta
        ));
    }
}
