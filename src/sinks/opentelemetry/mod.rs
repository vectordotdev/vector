use crate::codecs::Encoder;
use crate::{
    codecs::{EncodingConfigWithFraming, Transformer},
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    sinks::{
        Healthcheck, VectorSink,
        http::config::{HttpMethod, HttpSinkConfig},
    },
};
use indoc::indoc;
use vector_config::component::GenerateConfig;
use vector_lib::codecs::encoding::{Framer, ProtobufSerializer, Serializer};
use vector_lib::opentelemetry::proto::{
    LOGS_REQUEST_MESSAGE_TYPE, METRICS_REQUEST_MESSAGE_TYPE, TRACES_REQUEST_MESSAGE_TYPE,
};
use vector_lib::{
    codecs::{
        JsonSerializerConfig,
        encoding::{FramingConfig, SerializerConfig},
    },
    configurable::configurable_component,
};

/// Configuration for the `OpenTelemetry` sink.
#[configurable_component(sink("opentelemetry", "Deliver OTLP data over HTTP."))]
#[derive(Clone, Debug, Default)]
pub struct OpenTelemetryConfig {
    /// Protocol configuration
    #[configurable(derived)]
    protocol: Protocol,

    /// Setting this field to `true`, will override all encoding settings and it will encode requests based on the
    /// [OpenTelemetry protocol](https://opentelemetry.io/docs/specs/otel/protocol/).
    ///
    /// The endpoint is used to determine the data type:
    /// * v1/logs → OTLP Logs
    /// * v1/traces → OTLP Traces
    /// * v1/metrics → OTLP Metrics
    ///
    /// More information available [here](https://opentelemetry.io/docs/specs/otlp/?utm_source=chatgpt.com#otlphttp-request).
    #[configurable(derived)]
    #[serde(default)]
    pub use_otlp_encoding: bool,
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
            Protocol::Http(config) => {
                if self.use_otlp_encoding {
                    let serializer = ProtobufSerializer::new_from_bytes(
                        vector_lib::opentelemetry::proto::DESCRIPTOR_BYTES,
                        to_message_type(&config.uri.to_string())?,
                    )?;
                    let encoder = Encoder::<Framer>::new(
                        FramingConfig::Bytes.build(),
                        Serializer::Protobuf(serializer),
                    );
                    config
                        .build_with_encoder(cx, encoder, config.encoding.transformer())
                        .await
                } else {
                    config.build(cx).await
                }
            }
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

/// Checks if an endpoint ends with a known OTEL proto request.
pub fn to_message_type(endpoint: &str) -> crate::Result<&'static str> {
    if endpoint.ends_with("v1/logs") {
        Ok(LOGS_REQUEST_MESSAGE_TYPE)
    } else if endpoint.ends_with("v1/traces") {
        Ok(TRACES_REQUEST_MESSAGE_TYPE)
    } else if endpoint.ends_with("v1/metrics") {
        Ok(METRICS_REQUEST_MESSAGE_TYPE)
    } else {
        Err(format!("Endpoint {endpoint} not supported, should end with 'v1/logs', 'v1/metrics' or 'v1/traces'.").into())
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::OpenTelemetryConfig>();
    }
}
