use indoc::indoc;
use vector_config::component::GenerateConfig;
use vector_lib::{codecs::encoding::SerializerConfig, configurable::configurable_component};

use crate::{
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext, ValidatedSink},
    sinks::{
        Healthcheck, VectorSink,
        http::config::{HttpSinkConfig, ValidatedHttp},
    },
};

/// Configuration for the `OpenTelemetry` sink.
#[configurable_component(sink("opentelemetry", "Deliver OTLP data over HTTP."))]
#[derive(Clone, Debug)]
pub struct OpenTelemetryConfig {
    /// Protocol configuration
    protocol: Protocol,
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

impl GenerateConfig for OpenTelemetryConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc! {r#"
            protocol:
              type: http
              uri: http://localhost:5318/v1/logs
              encoding:
                codec: json
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "opentelemetry")]
impl SinkConfig for OpenTelemetryConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        match &self.protocol {
            Protocol::Http(config) => Some(&config.confinement),
        }
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

#[async_trait::async_trait]
impl ValidatedSink for OpenTelemetryConfig {
    type Validated = ValidatedHttp;

    fn validate(&self) -> crate::Result<ValidatedHttp> {
        match &self.protocol {
            Protocol::Http(config) => config.validate(),
        }
    }

    async fn build(
        &self,
        validated: &ValidatedHttp,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        match &self.protocol {
            Protocol::Http(config) => {
                warn_on_invalid_otlp_batching(config);
                // Confinement of the URI and templated headers happens here (not in
                // `validate`) so per-template security warnings carry the outer
                // `opentelemetry` component type rather than `http`.
                config.build_from_validated(validated, cx, Self::NAME).await
            }
        }
    }
}

fn warn_on_invalid_otlp_batching(config: &HttpSinkConfig) {
    let (_, serializer) = config.encoding.config();
    let is_json = matches!(serializer, SerializerConfig::Json(_));
    let batches_more_than_one = !matches!(config.batch.max_events, Some(1));
    if is_json && batches_more_than_one {
        tracing::warn!(
            message = "`opentelemetry` sink is configured with `encoding.codec = json` and \
                       `batch.max_events` greater than 1. This produces invalid OTLP request \
                       bodies that receivers reject with HTTP 400. Use `encoding.codec = otlp` \
                       (recommended) or set `batch.max_events = 1`. See \
                       https://github.com/vectordotdev/vector/issues/22054.",
        );
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::OpenTelemetryConfig>();
    }

    #[test]
    fn validate_produces_usable_state() {
        let config: OpenTelemetryConfig =
            serde_json::from_value(OpenTelemetryConfig::generate_config()).unwrap();
        config.validate().expect("validation should succeed");
    }
}
