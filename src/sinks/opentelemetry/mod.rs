mod grpc;

#[cfg(all(test, feature = "opentelemetry-integration-tests"))]
mod integration_tests;

use indoc::indoc;
use vector_config::component::GenerateConfig;
use vector_lib::configurable::configurable_component;

use crate::{
    codecs::EncodingConfigWithFraming,
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    http::Auth,
    sinks::{
        Healthcheck, VectorSink,
        http::config::{HttpMethod, HttpSinkConfig},
        util::{
            BatchConfig, Compression, RealtimeEventBasedDefaultBatchSettings,
            RealtimeSizeBasedDefaultBatchSettings, http::RequestConfig,
        },
    },
    template::Template,
    tls::TlsConfig,
};

pub use grpc::{GrpcCompression, GrpcSinkConfig};

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
        batch: BatchConfig<RealtimeEventBasedDefaultBatchSettings>,
    },
}

/// Configuration for the `opentelemetry` sink.
#[configurable_component(sink("opentelemetry", "Deliver OTLP data over HTTP or gRPC."))]
#[derive(Clone, Debug)]
pub struct OpenTelemetryConfig {
    /// The transport protocol to use. Defaults to `http`.
    #[configurable(derived)]
    #[serde(flatten)]
    pub protocol: OtlpProtocol,

    /// The URI to send requests to.
    ///
    /// Supports template syntax (e.g. `http://{{ host }}:4318/v1/logs`).
    /// Must include a scheme (`http://` or `https://`) and a port.
    ///
    /// # Examples
    ///
    /// - `http://localhost:5318/v1/logs` (HTTP)
    /// - `http://localhost:4317` (gRPC)
    #[configurable(metadata(docs::examples = "http://localhost:5318/v1/logs"))]
    #[configurable(metadata(docs::examples = "http://localhost:4317"))]
    pub uri: Template,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Compression,

    #[configurable(derived)]
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
                    headers: None,
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
                    other => {
                        return Err(format!(
                            "gRPC transport only supports 'none' or 'gzip' compression, got '{other}'"
                        )
                        .into())
                    }
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
            OtlpProtocol::Grpc { .. } => Input::all(),
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
