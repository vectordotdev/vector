mod grpc;

#[cfg(all(test, feature = "opentelemetry-integration-tests"))]
mod integration_tests;

use indoc::indoc;
use serde::Deserialize;
use vector_config::component::GenerateConfig;
use vector_lib::{codecs::encoding::SerializerConfig, configurable::configurable_component};

use crate::{
    codecs::EncodingConfigWithFraming,
    config::{AcknowledgementsConfig, DataType, Input, SinkConfig, SinkContext},
    http::Auth,
    sinks::{
        Healthcheck, VectorSink,
        http::config::{HttpMethod, HttpSinkConfig},
        util::{
            BatchConfig, Compression, RealtimeEventBasedDefaultBatchSettings,
            RealtimeSizeBasedDefaultBatchSettings,
            http::{RequestConfig, RetryStrategy},
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

        #[configurable(derived)]
        #[serde(default)]
        retry_strategy: RetryStrategy,
    },

    /// Send OTLP data over gRPC.
    Grpc {
        #[configurable(derived)]
        #[serde(default)]
        batch: BatchConfig<RealtimeEventBasedDefaultBatchSettings>,
    },
}

/// Configuration for the `opentelemetry` sink.
#[configurable_component(
    sink("opentelemetry", "Deliver OTLP data over HTTP or gRPC."),
    no_deser
)]
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
    #[configurable(metadata(
        docs::warnings = "The `grpc` protocol only supports `none` and `gzip`. Specifying any other algorithm causes Vector to fail at startup."
    ))]
    #[serde(default)]
    pub compression: Compression,

    #[configurable(derived)]
    #[configurable(metadata(
        docs::description = "Outbound request settings for retry, concurrency, timeout, and headers. \
        For the `grpc` protocol, `request.headers` entries are forwarded as gRPC metadata — use them \
        for authentication (e.g. `authorization: \"Bearer <token>\"`) since the HTTP-only `auth` field \
        is not available for gRPC."
    ))]
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

/// Mirror of `OpenTelemetryConfig` with a plain serde derive, used to decode the new flat format.
#[derive(Debug, Deserialize)]
struct FlatOpenTelemetryConfig {
    #[serde(flatten)]
    protocol: OtlpProtocol,
    uri: Template,
    #[serde(default)]
    compression: Compression,
    #[serde(default)]
    request: RequestConfig,
    tls: Option<TlsConfig>,
    #[serde(default, deserialize_with = "crate::serde::bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
}

/// Legacy (pre-flattening) nested format: everything under `protocol.*` with `protocol.type`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyOpenTelemetryConfig {
    protocol: LegacyProtocol,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum LegacyProtocol {
    /// The legacy format embedded the full `HttpSinkConfig` (only HTTP was supported).
    Http(HttpSinkConfig),
}

impl From<FlatOpenTelemetryConfig> for OpenTelemetryConfig {
    fn from(flat: FlatOpenTelemetryConfig) -> Self {
        Self {
            protocol: flat.protocol,
            uri: flat.uri,
            compression: flat.compression,
            request: flat.request,
            tls: flat.tls,
            acknowledgements: flat.acknowledgements,
        }
    }
}

impl From<LegacyOpenTelemetryConfig> for OpenTelemetryConfig {
    fn from(legacy: LegacyOpenTelemetryConfig) -> Self {
        match legacy.protocol {
            LegacyProtocol::Http(http) => Self {
                protocol: OtlpProtocol::Http {
                    method: http.method,
                    auth: http.auth,
                    encoding: http.encoding,
                    payload_prefix: http.payload_prefix,
                    payload_suffix: http.payload_suffix,
                    batch: http.batch,
                    retry_strategy: http.retry_strategy,
                },
                uri: http.uri,
                compression: http.compression,
                request: http.request,
                tls: http.tls,
                acknowledgements: http.acknowledgements,
            },
        }
    }
}

impl<'de> serde::Deserialize<'de> for OpenTelemetryConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let value = serde_json::Value::deserialize(deserializer)?;

        let is_legacy = matches!(value.get("protocol"), Some(serde_json::Value::Object(_)));

        if is_legacy {
            let legacy: LegacyOpenTelemetryConfig =
                serde_json::from_value(value).map_err(D::Error::custom)?;
            warn!(
                message = "The nested `protocol.*` configuration format for the `opentelemetry` \
                           sink is deprecated and will be removed. Migrate to the flat format: \
                           move all fields from `protocol.*` to the top level and replace \
                           `protocol.type` with `protocol`.",
            );
            Ok(legacy.into())
        } else {
            if matches!(
                value.get("protocol"),
                Some(serde_json::Value::String(protocol)) if protocol == "grpc"
            ) && value.get("retry_strategy").is_some()
            {
                return Err(D::Error::custom(
                    "`retry_strategy` is only valid when `protocol` is `http`",
                ));
            }

            let flat: FlatOpenTelemetryConfig =
                serde_json::from_value(value).map_err(D::Error::custom)?;
            Ok(flat.into())
        }
    }
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
                retry_strategy,
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
                    retry_strategy: retry_strategy.clone(),
                };
                warn_on_invalid_otlp_batching(&config);
                config.build(cx).await
            }
            OtlpProtocol::Grpc { batch } => {
                let grpc_compression = match self.compression {
                    Compression::None => GrpcCompression::None,
                    Compression::Gzip(_) => GrpcCompression::Gzip,
                    other => return Err(format!(
                        "gRPC transport only supports 'none' or 'gzip' compression, got '{other}'"
                    )
                    .into()),
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
    use http::StatusCode;
    use serde_json::json;

    use super::*;

    fn config_to_json(config: &OpenTelemetryConfig) -> serde_json::Value {
        serde_json::to_value(config).expect("config should serialize to JSON")
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::OpenTelemetryConfig>();
    }

    #[test]
    fn flat_http_format_parses() {
        let config: OpenTelemetryConfig = toml::from_str(indoc! {r#"
            protocol = "http"
            uri = "http://localhost:8889/write"
            method = "post"
            batch.max_events = 1
            encoding.codec = "json"
            framing.method = "bytes"
            [request.headers]
            content-type = "application/json"
        "#})
        .unwrap();

        assert_eq!(config.uri.to_string(), "http://localhost:8889/write");
        match config.protocol {
            OtlpProtocol::Http {
                method,
                batch,
                retry_strategy,
                ..
            } => {
                assert_eq!(method, HttpMethod::Post);
                assert_eq!(batch.max_events, Some(1));
                assert_eq!(retry_strategy, RetryStrategy::Default);
            }
            OtlpProtocol::Grpc { .. } => panic!("expected HTTP protocol"),
        }
    }

    #[test]
    fn flat_grpc_format_parses() {
        let config: OpenTelemetryConfig = toml::from_str(indoc! {r#"
            protocol = "grpc"
            uri = "http://localhost:4317"
        "#})
        .unwrap();

        assert_eq!(config.uri.to_string(), "http://localhost:4317");
        assert!(matches!(config.protocol, OtlpProtocol::Grpc { .. }));
    }

    #[test]
    fn legacy_format_parses_and_maps_to_flat_equivalent() {
        let legacy: OpenTelemetryConfig = toml::from_str(indoc! {r#"
            [protocol]
            type = "http"
            uri = "http://localhost:8889/write"
            method = "post"
            batch.max_events = 1
            encoding.codec = "json"
            framing.method = "bytes"
            [protocol.request.headers]
            content-type = "application/json"
        "#})
        .unwrap();

        let flat: OpenTelemetryConfig = toml::from_str(indoc! {r#"
            protocol = "http"
            uri = "http://localhost:8889/write"
            method = "post"
            batch.max_events = 1
            encoding.codec = "json"
            framing.method = "bytes"
            [request.headers]
            content-type = "application/json"
        "#})
        .unwrap();

        assert_eq!(config_to_json(&legacy), config_to_json(&flat));
    }

    #[test]
    fn legacy_format_rejects_unknown_protocol_fields() {
        let err = toml::from_str::<OpenTelemetryConfig>(indoc! {r#"
            [protocol]
            type = "http"
            uri = "http://localhost:8889/write"
            encoding.codec = "json"
            unknown_field = true
        "#})
        .unwrap_err();

        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn legacy_format_rejects_mixed_top_level_fields() {
        let err = toml::from_str::<OpenTelemetryConfig>(indoc! {r#"
            uri = "http://localhost:8889/write"
            [protocol]
            type = "http"
            uri = "http://localhost:8889/write"
            encoding.codec = "json"
        "#})
        .unwrap_err();

        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn flat_format_error_on_missing_uri() {
        let err = toml::from_str::<OpenTelemetryConfig>(indoc! {r#"
            protocol = "http"
            encoding.codec = "json"
        "#})
        .unwrap_err();

        assert!(err.to_string().contains("uri"));
    }

    #[test]
    fn flat_format_error_on_invalid_protocol_type() {
        let err = toml::from_str::<OpenTelemetryConfig>(indoc! {r#"
            protocol = 123
            uri = "http://localhost:8889/write"
        "#})
        .unwrap_err();

        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn legacy_decoded_config_serializes_to_flat_format() {
        let config: OpenTelemetryConfig = toml::from_str(indoc! {r#"
            [protocol]
            type = "http"
            uri = "http://localhost:8889/write"
            method = "post"
            encoding.codec = "json"
        "#})
        .unwrap();

        let serialized = config_to_json(&config);
        assert_eq!(serialized.get("protocol"), Some(&json!("http")));
        assert!(serialized.get("protocol").unwrap().is_string());
        assert_eq!(
            serialized.get("uri"),
            Some(&json!("http://localhost:8889/write"))
        );
    }

    #[test]
    fn flat_http_retry_strategy_parses() {
        let config: OpenTelemetryConfig = toml::from_str(indoc! {r#"
            protocol = "http"
            uri = "http://localhost:8889/write"
            encoding.codec = "json"
            retry_strategy.type = "custom"
            retry_strategy.status_codes = [502]
        "#})
        .unwrap();

        match config.protocol {
            OtlpProtocol::Http { retry_strategy, .. } => {
                assert_eq!(
                    retry_strategy,
                    RetryStrategy::Custom {
                        status_codes: vec![StatusCode::BAD_GATEWAY],
                    }
                );
            }
            OtlpProtocol::Grpc { .. } => panic!("expected HTTP protocol"),
        }
    }

    #[test]
    fn legacy_retry_strategy_maps_to_flat_equivalent() {
        let legacy: OpenTelemetryConfig = toml::from_str(indoc! {r#"
            [protocol]
            type = "http"
            uri = "http://localhost:8889/write"
            encoding.codec = "json"
            retry_strategy.type = "custom"
            retry_strategy.status_codes = [502]
        "#})
        .unwrap();

        let flat: OpenTelemetryConfig = toml::from_str(indoc! {r#"
            protocol = "http"
            uri = "http://localhost:8889/write"
            encoding.codec = "json"
            retry_strategy.type = "custom"
            retry_strategy.status_codes = [502]
        "#})
        .unwrap();

        assert_eq!(config_to_json(&legacy), config_to_json(&flat));
    }

    #[test]
    fn flat_grpc_rejects_retry_strategy() {
        let err = toml::from_str::<OpenTelemetryConfig>(indoc! {r#"
            protocol = "grpc"
            uri = "http://localhost:4317"
            retry_strategy.type = "custom"
            retry_strategy.status_codes = [502]
        "#})
        .unwrap_err();

        assert!(err.to_string().contains("retry_strategy"));
    }
}
