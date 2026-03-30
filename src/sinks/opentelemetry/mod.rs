mod grpc;

#[cfg(all(test, feature = "opentelemetry-integration-tests"))]
mod integration_tests;

use indoc::indoc;
use vector_config::component::GenerateConfig;
use vector_lib::configurable::configurable_component;

use crate::{
    codecs::EncodingConfigWithFraming,
    config::{AcknowledgementsConfig, DataType, Input, SinkConfig, SinkContext},
    http::Auth,
    sinks::{
        Healthcheck, VectorSink,
        http::config::{HttpMethod, HttpSinkConfig},
        util::{
            BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings,
            http::RequestConfig,
        },
    },
    template::Template,
    tls::TlsConfig,
};

pub use grpc::GrpcCompression;
use grpc::GrpcSinkConfig;

/// Transport protocol for the OpenTelemetry sink.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "protocol", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
#[configurable(metadata(docs::enum_tag_description = "The transport protocol to use."))]
pub enum OtlpProtocol {
    /// Send OTLP data over HTTP.
    Http {
        /// The HTTP method to use. Defaults to `post`.
        #[serde(default)]
        method: HttpMethod,

        #[configurable(derived)]
        auth: Option<Auth>,

        /// Encoding configuration.
        #[configurable(derived)]
        #[serde(flatten)]
        encoding: EncodingConfigWithFraming,

        /// A string to prefix the payload with.
        #[serde(default)]
        payload_prefix: String,

        /// A string to suffix the payload with.
        #[serde(default)]
        payload_suffix: String,

        #[configurable(derived)]
        #[serde(default)]
        batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
    },

    /// Send OTLP data over gRPC.
    Grpc {
        #[configurable(derived)]
        #[serde(default)]
        batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
    },
}

/// Configuration for the `opentelemetry` sink.
#[configurable_component(sink("opentelemetry", "Deliver OTLP data over HTTP or gRPC."))]
#[derive(Clone, Debug)]
pub struct OpenTelemetryConfig {
    /// The transport protocol to use.
    #[configurable(derived)]
    #[serde(flatten)]
    pub protocol: OtlpProtocol,

    /// The URI to send requests to.
    ///
    /// Supports template syntax (e.g. `http://{{ host }}:4317`). Must include a scheme
    /// (`http://` or `https://`) and a port.
    ///
    /// For the gRPC transport, the template is rendered once per batch using the first event
    /// in the batch.
    ///
    /// # Examples
    ///
    /// - `http://localhost:5318/v1/logs` (HTTP)
    /// - `http://localhost:4317` (gRPC)
    #[configurable(metadata(docs::examples = "http://localhost:5318/v1/logs"))]
    #[configurable(metadata(docs::examples = "http://localhost:4317"))]
    #[configurable(metadata(
        docs::warnings = "When using template syntax, the rendered URI is taken from event data. Only use dynamic URIs with trusted event sources to avoid directing Vector to unintended internal network destinations."
    ))]
    pub uri: Template,

    #[configurable(derived)]
    #[configurable(metadata(docs::warnings = "The `grpc` protocol only supports `none` and `gzip`. Specifying any other algorithm causes Vector to fail at startup."))]
    #[serde(default)]
    pub compression: Compression,

    #[configurable(derived)]
    #[configurable(metadata(docs::description = "Outbound request settings for retry, concurrency, timeout, and headers. \
        For the `grpc` protocol, `request.headers` entries are forwarded as gRPC metadata — use them \
        for authentication (e.g. `authorization: \"Bearer <token>\"`) since the HTTP-only `auth` field \
        is not available for gRPC."))]
    #[serde(default)]
    pub request: RequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for OpenTelemetryConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            protocol = "http"
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
            OtlpProtocol::Http {
                method,
                auth,
                encoding,
                payload_prefix,
                payload_suffix,
                batch,
            } => {
                let config = HttpSinkConfig {
                    uri: self.uri.clone(),
                    method: *method,
                    auth: auth.clone(),
                    compression: self.compression,
                    encoding: encoding.clone(),
                    payload_prefix: payload_prefix.clone(),
                    payload_suffix: payload_suffix.clone(),
                    batch: *batch,
                    request: self.request.clone(),
                    tls: self.tls.clone(),
                    acknowledgements: self.acknowledgements,
                };
                config.build(cx).await
            }
            OtlpProtocol::Grpc { batch } => {
                let grpc_compression = match self.compression {
                    Compression::None => GrpcCompression::None,
                    Compression::Gzip(_) => GrpcCompression::Gzip,
                    other => return Err(format!(
                        "gRPC transport only supports 'none' or 'gzip' compression, got '{other}'"
                    ).into()),
                };
                let config = GrpcSinkConfig {
                    uri: self.uri.clone(),
                    compression: grpc_compression,
                    batch: *batch,
                    request: self.request.clone(),
                    tls: self.tls.clone(),
                    acknowledgements: self.acknowledgements,
                };
                config.build(cx).await
            }
        }
    }

    fn input(&self) -> Input {
        match &self.protocol {
            OtlpProtocol::Http { encoding, .. } => Input::new(encoding.config().1.input_type()),
            OtlpProtocol::Grpc { .. } => Input::new(DataType::Log | DataType::Trace),
        }
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::OpenTelemetryConfig>();
    }
}
