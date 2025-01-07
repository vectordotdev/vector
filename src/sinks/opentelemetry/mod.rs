use crate::codecs::{EncodingConfigWithFraming, Transformer};
use crate::config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext};
use crate::sinks::http::config::{HttpMethod, HttpSinkConfig};
use crate::sinks::{Healthcheck, VectorSink};
use indoc::indoc;
use vector_config::component::GenerateConfig;
use vector_lib::codecs::encoding::{FramingConfig, SerializerConfig};
use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::configurable::configurable_component;

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
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "opentelemetry")]
impl SinkConfig for OpenTelemetryConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        match &self.protocol {
            Protocol::Http(config) => config.build(cx).await,
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

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::OpenTelemetryConfig>();
    }
}
