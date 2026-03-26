use std::net::SocketAddr;

use crate::{
    config::{
        DataType, GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig,
        SourceContext, SourceOutput,
    },
    http::KeepaliveConfig,
    serde::bool_or_struct,
    sources::{
        Source,
        http_server::{build_param_matcher, remove_duplicates},
        opentelemetry::{
            grpc::Service,
            http::{build_warp_filter, run_http_server},
        },
        util::grpc::run_grpc_server_with_routes,
    },
};
use futures::FutureExt;
use futures_util::{TryFutureExt, future::join};
use tonic::{codec::CompressionEncoding, transport::server::RoutesBuilder};
use vector_config::indexmap::IndexSet;
use vector_lib::{
    codecs::decoding::{OtlpDeserializer, OtlpSignalType},
    config::{LegacyKey, LogNamespace, log_schema},
    configurable::configurable_component,
    internal_event::{BytesReceived, EventsReceived, Protocol},
    lookup::{OwnedTargetPath, owned_value_path},
    opentelemetry::{
        logs::{
            ATTRIBUTES_KEY, DROPPED_ATTRIBUTES_COUNT_KEY, FLAGS_KEY, OBSERVED_TIMESTAMP_KEY,
            RESOURCE_KEY, SEVERITY_NUMBER_KEY, SEVERITY_TEXT_KEY, SPAN_ID_KEY, TRACE_ID_KEY,
        },
        proto::collector::{
            logs::v1::logs_service_server::LogsServiceServer,
            metrics::v1::metrics_service_server::MetricsServiceServer,
            trace::v1::trace_service_server::TraceServiceServer,
        },
    },
    schema::Definition,
    tls::{MaybeTlsSettings, TlsEnableableConfig},
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
    /// Converts a boolean value to an OtlpDecodingConfig.
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
    pub const fn any_enabled(&self) -> bool {
        self.logs || self.metrics || self.traces
    }

    /// Returns true if all signals are configured to use OTLP decoding.
    pub const fn all_enabled(&self) -> bool {
        self.logs && self.metrics && self.traces
    }

    /// Returns true if signals have mixed configuration (some enabled, some disabled).
    pub const fn is_mixed(&self) -> bool {
        self.any_enabled() && !self.all_enabled()
    }
}

/// Configuration for the `opentelemetry` source.
#[configurable_component(source("opentelemetry", "Receive OTLP data through gRPC or HTTP."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryConfig {
    #[configurable(derived)]
    pub grpc: GrpcConfig,

    #[configurable(derived)]
    pub http: HttpConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: SourceAcknowledgementsConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

    /// Configuration for OTLP decoding behavior.
    ///
    /// This configuration controls how OpenTelemetry Protocol (OTLP) data is decoded for each
    /// signal type (logs, metrics, traces). When a signal is configured to use OTLP decoding, the raw OTLP format is
    /// preserved, allowing the data to be forwarded to downstream OTLP collectors without transformation.
    /// Otherwise, the signal is converted to Vector's native event format.
    ///
    /// Simple boolean form:
    ///
    /// ```yaml
    /// use_otlp_decoding: true  # All signals preserve OTLP format
    /// # or
    /// use_otlp_decoding: false # All signals use Vector native format (default)
    /// ```
    ///
    /// Per-signal configuration:
    ///
    /// ```yaml
    /// use_otlp_decoding:
    ///   logs: false     # Convert to Vector native format
    ///   metrics: false  # Convert to Vector native format
    ///   traces: true    # Preserve OTLP format
    /// ```
    ///
    /// **Note:** When OTLP decoding is enabled for metrics:
    /// - Metrics are parsed as logs while preserving the OTLP format
    /// - Vector's metric transforms will NOT be compatible with this output
    /// - The events can be forwarded directly (passthrough) to a downstream OTLP collector
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub use_otlp_decoding: OtlpDecodingConfig,
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

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsEnableableConfig>,
}

fn example_grpc_config() -> GrpcConfig {
    GrpcConfig {
        address: "0.0.0.0:4317".parse().unwrap(),
        tls: None,
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

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
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
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            grpc: example_grpc_config(),
            http: example_http_config(),
            acknowledgements: Default::default(),
            log_namespace: None,
            use_otlp_decoding: OtlpDecodingConfig::default(),
        })
        .unwrap()
    }
}

impl OpentelemetryConfig {
    pub(crate) fn get_signal_deserializer(
        &self,
        signal_type: OtlpSignalType,
    ) -> vector_common::Result<Option<OtlpDeserializer>> {
        let should_use_otlp = match signal_type {
            OtlpSignalType::Logs => self.use_otlp_decoding.logs,
            OtlpSignalType::Metrics => self.use_otlp_decoding.metrics,
            OtlpSignalType::Traces => self.use_otlp_decoding.traces,
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

#[async_trait::async_trait]
#[typetag::serde(name = "opentelemetry")]
impl SourceConfig for OpentelemetryConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);
        let events_received = register!(EventsReceived);
        let log_namespace = cx.log_namespace(self.log_namespace);

        let grpc_tls_settings = MaybeTlsSettings::from_config(self.grpc.tls.as_ref(), true)?;

        // Log info message when using mixed OTLP decoding formats
        if self.use_otlp_decoding.is_mixed() {
            info!(
                message = "Signals with OTLP decoding enabled will preserve raw format; others will use Vector native format.",
                logs_otlp = self.use_otlp_decoding.logs,
                metrics_otlp = self.use_otlp_decoding.metrics,
                traces_otlp = self.use_otlp_decoding.traces,
            );
        }

        let logs_deserializer = self.get_signal_deserializer(OtlpSignalType::Logs)?;
        let metrics_deserializer = self.get_signal_deserializer(OtlpSignalType::Metrics)?;
        let traces_deserializer = self.get_signal_deserializer(OtlpSignalType::Traces)?;

        let log_service = LogsServiceServer::new(Service {
            pipeline: cx.out.clone(),
            acknowledgements,
            log_namespace,
            events_received: events_received.clone(),
            deserializer: logs_deserializer.clone(),
        })
        .accept_compressed(CompressionEncoding::Gzip)
        .max_decoding_message_size(usize::MAX);

        let metrics_service = MetricsServiceServer::new(Service {
            pipeline: cx.out.clone(),
            acknowledgements,
            log_namespace,
            events_received: events_received.clone(),
            deserializer: metrics_deserializer.clone(),
        })
        .accept_compressed(CompressionEncoding::Gzip)
        .max_decoding_message_size(usize::MAX);

        let trace_service = TraceServiceServer::new(Service {
            pipeline: cx.out.clone(),
            acknowledgements,
            log_namespace,
            events_received: events_received.clone(),
            deserializer: traces_deserializer.clone(),
        })
        .accept_compressed(CompressionEncoding::Gzip)
        .max_decoding_message_size(usize::MAX);

        let mut builder = RoutesBuilder::default();
        builder
            .add_service(log_service)
            .add_service(metrics_service)
            .add_service(trace_service);

        let grpc_source = run_grpc_server_with_routes(
            self.grpc.address,
            grpc_tls_settings,
            builder.routes(),
            cx.shutdown.clone(),
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
            filters,
            cx.shutdown,
            self.http.keepalive.clone(),
        )
        .map_err(|error| {
            error!(message = "OpenTelemetry source HTTP server failed.", %error);
        });

        Ok(join(grpc_source, http_source).map(|_| Ok(())).boxed())
    }

    // TODO: appropriately handle "severity" meaning across both "severity_text" and "severity_number",
    // as both are optional and can be converted to/from.
    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = Definition::new_with_default_metadata(Kind::any(), [log_namespace])
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

        let logs_output = if self.use_otlp_decoding.logs {
            SourceOutput::new_maybe_logs(DataType::Log, Definition::any()).with_port(LOGS)
        } else {
            SourceOutput::new_maybe_logs(DataType::Log, schema_definition).with_port(LOGS)
        };

        let metrics_output = if self.use_otlp_decoding.metrics {
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
