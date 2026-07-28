use indoc::indoc;
use vector_config::component::GenerateConfig;
use vector_lib::{
    codecs::{
        JsonSerializerConfig,
        encoding::{FramingConfig, SerializerConfig},
    },
    configurable::configurable_component,
};

use crate::{
    codecs::{EncodingConfigWithFraming, Transformer},
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    sinks::{
        Healthcheck, VectorSink,
        http::config::{HttpMethod, HttpSinkConfig},
    },
};

/// Configuration for the `OpenTelemetry` sink.
#[configurable_component(sink("opentelemetry", "Deliver OTLP data over HTTP."))]
#[derive(Clone, Debug, Default)]
pub struct OpenTelemetryConfig {
    /// Protocol configuration
    #[configurable(derived)]
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
            compression: Default::default(),
            payload_prefix: Default::default(),
            payload_suffix: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            tls: Default::default(),
            acknowledgements: Default::default(),
            retry_strategy: Default::default(),
            confinement: Default::default(),
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
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "opentelemetry")]
impl SinkConfig for OpenTelemetryConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        match &self.protocol {
            Protocol::Http(config) => {
                warn_on_invalid_otlp_batching(config);
                // Delegate to the HTTP sink, but thread through `opentelemetry`
                // as the component type so security warnings carry the outer
                // sink type rather than `http`.
                config.build_with_component_type(cx, Self::NAME).await
            }
        }
    }

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
        match self.protocol {
            Protocol::Http(ref config) => config.acknowledgements(),
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
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::OpenTelemetryConfig>();
    }
}
