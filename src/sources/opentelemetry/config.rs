use std::{net::SocketAddr, num::NonZeroUsize, time::Duration};

use crate::{
    config::{
        DataType, GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig,
        SourceContext, SourceOutput,
    },
    http::KeepaliveConfig,
    sources::{
        Source,
        http_server::{build_param_matcher, remove_duplicates},
        opentelemetry::{
            grpc::{Service, middleware_error_response as grpc_error_response},
            http::{
                build_warp_filter, middleware_error_response as http_error_response,
                run_http_server,
            },
            request_control::RequestControl,
        },
        util::{
            decompression::max_decompressed_size_bytes,
            grpc::{GrpcKeepaliveConfig, run_grpc_server_with_routes},
        },
    },
};
use futures::FutureExt;
use futures_util::{TryFutureExt, future::join};
use serde::{Deserialize, Deserializer, Serialize, de};
use tokio::sync::Semaphore;
use tonic::transport::server::RoutesBuilder;
use vector_config::indexmap::IndexSet;
use vector_lib::{
    codecs::decoding::{OtlpDeserializer, OtlpDeserializerConfig, OtlpSignalType},
    config::{LegacyKey, LogNamespace, log_schema},
    configurable::configurable_component,
    internal_event::{BytesReceived, EventsReceived, Protocol},
    lookup::{OwnedTargetPath, owned_value_path},
    opentelemetry::{
        logs::{
            ATTRIBUTES_KEY, DROPPED_ATTRIBUTES_COUNT_KEY, EVENT_NAME_KEY, FLAGS_KEY,
            OBSERVED_TIMESTAMP_KEY, RESOURCE_KEY, SEVERITY_NUMBER_KEY, SEVERITY_TEXT_KEY,
            SPAN_ID_KEY, TRACE_ID_KEY,
        },
        proto::collector::{
            logs::v1::logs_service_server::LogsServiceServer,
            metrics::v1::metrics_service_server::MetricsServiceServer,
            trace::v1::trace_service_server::TraceServiceServer,
        },
    },
    schema::Definition,
    tls::{MaybeTlsSettings, TlsAcceptorReloader, TlsEnableableConfig},
};
use vrl::value::{Kind, kind::Collection};

pub const LOGS: &str = "logs";
pub const METRICS: &str = "metrics";
pub const TRACES: &str = "traces";

/// Configuration for OTLP decoding behavior.
#[configurable_component]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OtlpDecodingConfig {
    /// Whether to use OTLP decoding for logs.
    ///
    /// When `true`, logs preserve their OTLP format.
    /// When `false` (default), logs are converted to Vector's native format.
    #[serde(default)]
    pub logs: bool,

    /// Whether to use OTLP decoding for metrics.
    ///
    /// When `true`, metrics preserve their OTLP format but are processed as logs.
    /// When `false` (default), metrics are converted to Vector's native metric format.
    #[serde(default)]
    pub metrics: bool,

    /// Whether to use OTLP decoding for traces.
    ///
    /// When `true`, traces preserve their OTLP format.
    /// When `false` (default), traces are converted to Vector's native format.
    #[serde(default)]
    pub traces: bool,
}

impl From<bool> for OtlpDecodingConfig {
    /// Converts a boolean value to an `OtlpDecodingConfig`.
    ///
    /// This provides backward compatibility with the previous boolean configuration.
    /// - `true` enables OTLP decoding for all signals
    /// - `false` disables OTLP decoding for all signals (uses Vector native format)
    fn from(value: bool) -> Self {
        Self {
            logs: value,
            metrics: value,
            traces: value,
        }
    }
}

impl OtlpDecodingConfig {
    /// Returns true if any signal is configured to use OTLP decoding.
    #[must_use]
    pub const fn any_enabled(&self) -> bool {
        self.logs || self.metrics || self.traces
    }

    /// Returns true if all signals are configured to use OTLP decoding.
    #[must_use]
    pub const fn all_enabled(&self) -> bool {
        self.logs && self.metrics && self.traces
    }

    /// Returns true if signals have mixed configuration (some enabled, some disabled).
    #[must_use]
    pub const fn is_mixed(&self) -> bool {
        self.any_enabled() && !self.all_enabled()
    }
}

/// Selects which signals retain their OTLP representation.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "codec", rename_all = "snake_case", deny_unknown_fields)]
#[configurable(metadata(
    docs::enum_tag_description = "The codec to use. Only `otlp` is supported."
))]
pub enum DecodingConfig {
    /// Preserves the OTLP representation of the selected signals.
    Otlp {
        /// Signals that retain their OTLP representation.
        ///
        /// Unlisted signals are still accepted and converted to Vector's native representation.
        /// Omitting this list selects all signals. An empty list uses native conversion for all
        /// signals. Order has no effect, and duplicate signal names are ignored.
        /// OTLP metrics are emitted as log events and cannot be used with native metric transforms.
        #[serde(default = "default_signal_types")]
        signal_types: IndexSet<OtlpSignalType>,
    },
}

fn default_signal_types() -> IndexSet<OtlpSignalType> {
    OtlpDeserializerConfig::default().signal_types
}

/// Legacy configuration for selecting OTLP representation.
#[configurable_component(no_ser)]
#[derive(Clone, Debug)]
#[serde(untagged)]
pub enum LegacyOtlpDecodingConfig {
    /// Selects the same representation for all signals.
    Boolean(bool),
    /// Selects the representation of each signal separately.
    PerSignal(OtlpDecodingConfig),
}

impl Serialize for LegacyOtlpDecodingConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Keep boolean and object inputs equivalent in normalized configurations.
        match self {
            Self::Boolean(value) => OtlpDecodingConfig::from(*value).serialize(serializer),
            Self::PerSignal(config) => config.serialize(serializer),
        }
    }
}

impl From<bool> for LegacyOtlpDecodingConfig {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<OtlpDecodingConfig> for LegacyOtlpDecodingConfig {
    fn from(value: OtlpDecodingConfig) -> Self {
        Self::PerSignal(value)
    }
}

/// Configuration for the `opentelemetry` source.
#[configurable_component(source("opentelemetry", "Receive OTLP data through gRPC or HTTP."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryConfig {
    pub grpc: GrpcConfig,

    pub http: HttpConfig,

    #[serde(default, deserialize_with = "crate::serde::bool_or_struct")]
    pub acknowledgements: SourceAcknowledgementsConfig,

    /// Maximum number of requests processed concurrently across the HTTP and gRPC servers.
    ///
    /// Requests beyond this limit are rejected. When omitted, no request concurrency limit is enforced.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_max_concurrent_requests"
    )]
    pub max_concurrent_requests: Option<NonZeroUsize>,

    /// Maximum time spent processing a request through submission to the source output.
    ///
    /// When omitted, no request processing timeout is enforced. Downstream acknowledgement waiting
    /// is not subject to this timeout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    pub request_timeout_secs: Option<u64>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

    /// Decoding configuration for the signals received by this source.
    ///
    /// Set `codec` to `otlp` to preserve the OTLP representation. When this option is omitted,
    /// all signals are converted to Vector's native representation. Only the `otlp` codec is
    /// supported. Do not set this option together with `use_otlp_decoding`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decoding: Option<DecodingConfig>,

    /// Deprecated selection of OTLP decoding behavior.
    ///
    /// Use `decoding.codec: otlp` instead. To select individual signals, set
    /// `decoding.signal_types`. Remove this option to use native conversion for all signals.
    /// Existing boolean and object values remain supported during migration.
    /// See the [migration guide](/deprecations/#opentelemetry-use_otlp_decoding).
    #[configurable(deprecated)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_otlp_decoding: Option<LegacyOtlpDecodingConfig>,
}

fn deserialize_max_concurrent_requests<'de, D>(
    deserializer: D,
) -> Result<Option<NonZeroUsize>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<NonZeroUsize>::deserialize(deserializer)?;
    if value.is_some_and(|limit| limit.get() > Semaphore::MAX_PERMITS) {
        return Err(de::Error::custom(format!(
            "max_concurrent_requests must not exceed {}",
            Semaphore::MAX_PERMITS
        )));
    }
    Ok(value)
}

/// Configuration for the `opentelemetry` gRPC server.
#[configurable_component]
#[configurable(metadata(docs::examples = "example_grpc_config()"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct GrpcConfig {
    /// The socket address to listen for connections on.
    ///
    /// It _must_ include a port.
    #[configurable(metadata(docs::examples = "0.0.0.0:4317", docs::examples = "localhost:4317"))]
    pub address: SocketAddr,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsEnableableConfig>,

    #[serde(default)]
    pub keepalive: GrpcKeepaliveConfig,
}

fn example_grpc_config() -> GrpcConfig {
    GrpcConfig {
        address: "0.0.0.0:4317".parse().unwrap(),
        tls: None,
        keepalive: GrpcKeepaliveConfig::default(),
    }
}

/// Configuration for the `opentelemetry` HTTP server.
#[configurable_component]
#[configurable(metadata(docs::examples = "example_http_config()"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    /// The socket address to listen for connections on.
    ///
    /// It _must_ include a port.
    #[configurable(metadata(docs::examples = "0.0.0.0:4318", docs::examples = "localhost:4318"))]
    pub address: SocketAddr,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsEnableableConfig>,

    #[serde(default)]
    pub keepalive: KeepaliveConfig,

    /// A list of HTTP headers to include in the event.
    ///
    /// Accepts the wildcard (`*`) character for headers matching a specified pattern.
    ///
    /// Specifying "*" results in all headers included in the event.
    ///
    /// For log events in legacy namespace mode, headers are not included if a field with a conflicting name exists.
    /// For metrics and traces, headers are always added to event metadata.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "User-Agent"))]
    #[configurable(metadata(docs::examples = "X-My-Custom-Header"))]
    #[configurable(metadata(docs::examples = "X-*"))]
    #[configurable(metadata(docs::examples = "*"))]
    pub headers: Vec<String>,
}

fn example_http_config() -> HttpConfig {
    HttpConfig {
        address: "0.0.0.0:4318".parse().unwrap(),
        tls: None,
        keepalive: KeepaliveConfig::default(),
        headers: vec![],
    }
}

impl GenerateConfig for OpentelemetryConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            grpc: example_grpc_config(),
            http: example_http_config(),
            acknowledgements: SourceAcknowledgementsConfig::default(),
            max_concurrent_requests: None,
            request_timeout_secs: None,
            log_namespace: None,
            decoding: None,
            use_otlp_decoding: None,
        })
        .unwrap()
    }
}

impl OpentelemetryConfig {
    pub(crate) fn otlp_decoding(&self) -> OtlpDecodingConfig {
        if let Some(DecodingConfig::Otlp { signal_types }) = &self.decoding {
            return OtlpDecodingConfig {
                logs: signal_types.contains(&OtlpSignalType::Logs),
                metrics: signal_types.contains(&OtlpSignalType::Metrics),
                traces: signal_types.contains(&OtlpSignalType::Traces),
            };
        }
        match &self.use_otlp_decoding {
            Some(LegacyOtlpDecodingConfig::Boolean(value)) => (*value).into(),
            Some(LegacyOtlpDecodingConfig::PerSignal(config)) => config.clone(),
            None => OtlpDecodingConfig::default(),
        }
    }

    #[allow(
        clippy::unnecessary_wraps,
        reason = "Removing this fallible API is deferred to keep source construction and error handling unchanged. https://github.com/vectordotdev/vector/issues/23659"
    )]
    pub(crate) fn get_signal_deserializer(
        &self,
        signal_type: OtlpSignalType,
    ) -> vector_common::Result<Option<OtlpDeserializer>> {
        let decoding = self.otlp_decoding();
        let should_use_otlp = match signal_type {
            OtlpSignalType::Logs => decoding.logs,
            OtlpSignalType::Metrics => decoding.metrics,
            OtlpSignalType::Traces => decoding.traces,
        };

        if should_use_otlp {
            Ok(Some(OtlpDeserializer::new_with_signals(IndexSet::from([
                signal_type,
            ]))))
        } else {
            Ok(None)
        }
    }
}

impl OpentelemetryConfig {
    ///
    /// # Errors
    ///
    /// Returns an error if TLS, decoding, request limits, or header matching cannot be configured.
    /// Build the source serving runtime-swappable TLS acceptors for the gRPC and/or HTTP listeners.
    pub fn build_with_tls_reloaders(
        &self,
        cx: SourceContext,
        grpc_tls_reloader: Option<TlsAcceptorReloader>,
        http_tls_reloader: Option<TlsAcceptorReloader>,
    ) -> crate::Result<Source> {
        if self.decoding.is_some() && self.use_otlp_decoding.is_some() {
            return Err("`decoding` and `use_otlp_decoding` cannot be set together".into());
        }
        if self.use_otlp_decoding.is_some() {
            warn!(
                "DEPRECATED: `use_otlp_decoding` is deprecated. Use `decoding.codec: otlp` and `decoding.signal_types`, or omit both options for native conversion. See https://vector.dev/deprecations/#opentelemetry-use_otlp_decoding for migration instructions."
            );
        }
        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);
        let events_received = register!(EventsReceived);
        let request_control = RequestControl::new(
            self.max_concurrent_requests.map(NonZeroUsize::get),
            self.request_timeout_secs.map(Duration::from_secs),
        );
        let log_namespace = cx.log_namespace(self.log_namespace);

        let grpc_tls_settings = MaybeTlsSettings::from_config(self.grpc.tls.as_ref(), true)?;

        // Log info message when using mixed OTLP decoding formats
        let decoding = self.otlp_decoding();
        if decoding.is_mixed() {
            info!(
                message = "Signals with OTLP decoding enabled will preserve raw format; others will use Vector native format.",
                logs_otlp = decoding.logs,
                metrics_otlp = decoding.metrics,
                traces_otlp = decoding.traces,
            );
        }

        let logs_deserializer = self.get_signal_deserializer(OtlpSignalType::Logs)?;
        let metrics_deserializer = self.get_signal_deserializer(OtlpSignalType::Metrics)?;
        let traces_deserializer = self.get_signal_deserializer(OtlpSignalType::Traces)?;

        // Compression negotiation (gzip, zstd) is handled centrally by
        // `DecompressionAndMetricsLayer` in `sources::util::grpc`, so these
        // services deliberately do not call `.accept_compressed(..)`.
        let log_service = LogsServiceServer::new(Service {
            pipeline: cx.out.clone(),
            acknowledgements,
            log_namespace,
            events_received: events_received.clone(),
            deserializer: logs_deserializer.clone(),
        })
        .max_decoding_message_size(max_decompressed_size_bytes());

        let metrics_service = MetricsServiceServer::new(Service {
            pipeline: cx.out.clone(),
            acknowledgements,
            log_namespace,
            events_received: events_received.clone(),
            deserializer: metrics_deserializer.clone(),
        })
        .max_decoding_message_size(max_decompressed_size_bytes());

        let trace_service = TraceServiceServer::new(Service {
            pipeline: cx.out.clone(),
            acknowledgements,
            log_namespace,
            events_received: events_received.clone(),
            deserializer: traces_deserializer.clone(),
        })
        .max_decoding_message_size(max_decompressed_size_bytes());

        let mut builder = RoutesBuilder::default();
        builder
            .add_service(log_service)
            .add_service(metrics_service)
            .add_service(trace_service);

        let grpc_source = run_grpc_server_with_routes(
            self.grpc.address,
            grpc_tls_settings,
            grpc_tls_reloader,
            builder.routes(),
            self.grpc.keepalive.clone(),
            cx.shutdown.clone(),
            request_control.grpc_layer(grpc_error_response),
        )
        .map_err(|error| {
            error!(message = "OpenTelemetry source gRPC server failed.", %error);
        });

        let http_tls_settings = MaybeTlsSettings::from_config(self.http.tls.as_ref(), true)?;
        let protocol = http_tls_settings.http_protocol_name();
        let bytes_received = register!(BytesReceived::from(Protocol::from(protocol)));
        let headers =
            build_param_matcher(&remove_duplicates(self.http.headers.clone(), "headers"))?;

        let filters = build_warp_filter(
            acknowledgements,
            log_namespace,
            cx.out,
            bytes_received,
            events_received,
            headers,
            logs_deserializer,
            metrics_deserializer,
            traces_deserializer,
        );

        let http_source = run_http_server(
            self.http.address,
            http_tls_settings,
            http_tls_reloader,
            filters,
            cx.shutdown,
            self.http.keepalive.clone(),
            request_control.http_layer(http_error_response),
        )
        .map_err(|error| {
            error!(message = "OpenTelemetry source HTTP server failed.", %error);
        });

        Ok(join(grpc_source, http_source).map(|_| Ok(())).boxed())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "opentelemetry")]
impl SourceConfig for OpentelemetryConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        self.build_with_tls_reloaders(cx, None, None)
    }

    // TODO: appropriately handle "severity" meaning across both "severity_text" and "severity_number",
    // as both are optional and can be converted to/from.
    #[allow(
        clippy::too_many_lines,
        reason = "Splitting this source routine is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
    )]
    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = Definition::new_with_default_metadata(Kind::any(), [log_namespace])
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(EVENT_NAME_KEY))),
                &owned_value_path!(EVENT_NAME_KEY),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(RESOURCE_KEY))),
                &owned_value_path!(RESOURCE_KEY),
                Kind::object(Collection::from_unknown(Kind::any())).or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(ATTRIBUTES_KEY))),
                &owned_value_path!(ATTRIBUTES_KEY),
                Kind::object(Collection::from_unknown(Kind::any())).or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(TRACE_ID_KEY))),
                &owned_value_path!(TRACE_ID_KEY),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(SPAN_ID_KEY))),
                &owned_value_path!(SPAN_ID_KEY),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(SEVERITY_TEXT_KEY))),
                &owned_value_path!(SEVERITY_TEXT_KEY),
                Kind::bytes().or_undefined(),
                Some("severity"),
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(SEVERITY_NUMBER_KEY))),
                &owned_value_path!(SEVERITY_NUMBER_KEY),
                Kind::integer().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(FLAGS_KEY))),
                &owned_value_path!(FLAGS_KEY),
                Kind::integer().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(
                    DROPPED_ATTRIBUTES_COUNT_KEY
                ))),
                &owned_value_path!(DROPPED_ATTRIBUTES_COUNT_KEY),
                Kind::integer(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(
                    OBSERVED_TIMESTAMP_KEY
                ))),
                &owned_value_path!(OBSERVED_TIMESTAMP_KEY),
                Kind::timestamp(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                None,
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some("timestamp"),
            )
            .with_standard_vector_source_metadata();

        let schema_definition = match log_namespace {
            LogNamespace::Vector => {
                schema_definition.with_meaning(OwnedTargetPath::event_root(), "message")
            }
            LogNamespace::Legacy => {
                schema_definition.with_meaning(log_schema().owned_message_path(), "message")
            }
        };

        let decoding = self.otlp_decoding();
        let logs_output = if decoding.logs {
            SourceOutput::new_maybe_logs(DataType::Log, Definition::any()).with_port(LOGS)
        } else {
            SourceOutput::new_maybe_logs(DataType::Log, schema_definition).with_port(LOGS)
        };

        let metrics_output = if decoding.metrics {
            SourceOutput::new_maybe_logs(DataType::Log, Definition::any()).with_port(METRICS)
        } else {
            SourceOutput::new_metrics().with_port(METRICS)
        };

        vec![
            logs_output,
            metrics_output,
            SourceOutput::new_traces().with_port(TRACES),
        ]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![
            Resource::tcp(self.grpc.address),
            Resource::tcp(self.http.address),
        ]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}
