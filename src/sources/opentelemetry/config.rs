use std::net::SocketAddr;

use futures::FutureExt;
use futures_util::{future::join, TryFutureExt};

use tonic::{codec::CompressionEncoding, transport::server::RoutesBuilder};

use vector_lib::{
    config::{log_schema, LegacyKey, LogNamespace},
    configurable::configurable_component,
    internal_event::{BytesReceived, EventsReceived, Protocol},
    lookup::{owned_value_path, OwnedTargetPath},
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

use crate::{
    config::{
        DataType, GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig,
        SourceContext, SourceOutput,
    },
    http::KeepaliveConfig,
    serde::bool_or_struct,
    sources::{
        http_server::{build_param_matcher, remove_duplicates},
        opentelemetry::{
            grpc::Service,
            http::{build_warp_filter, run_http_server},
        },
        util::grpc::run_grpc_server_with_routes,
        Source,
    },
};

use vrl::value::{kind::Collection, Kind};

pub const LOGS: &str = "logs";
pub const METRICS: &str = "metrics";
pub const TRACES: &str = "traces";

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

    /// A list of HTTP headers to include in the log event.
    ///
    /// Accepts the wildcard (`*`) character for headers matching a specified pattern.
    ///
    /// Specifying "*" results in all headers included in the log event.
    ///
    /// These headers are not included in the JSON payload if a field with a conflicting name exists.
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
        })
        .unwrap()
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

        let log_service = LogsServiceServer::new(Service {
            pipeline: cx.out.clone(),
            acknowledgements,
            log_namespace,
            events_received: events_received.clone(),
        })
        .accept_compressed(CompressionEncoding::Gzip)
        .max_decoding_message_size(usize::MAX);

        let trace_service = TraceServiceServer::new(Service {
            pipeline: cx.out.clone(),
            acknowledgements,
            log_namespace,
            events_received: events_received.clone(),
        })
        .accept_compressed(CompressionEncoding::Gzip)
        .max_decoding_message_size(usize::MAX);

        let metrics_service = MetricsServiceServer::new(Service {
            pipeline: cx.out.clone(),
            acknowledgements,
            log_namespace,
            events_received: events_received.clone(),
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
            error!(message = "Source future failed.", %error);
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
        );
        let http_source = run_http_server(
            self.http.address,
            http_tls_settings,
            filters,
            cx.shutdown,
            self.http.keepalive.clone(),
        );

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

        vec![
            SourceOutput::new_maybe_logs(DataType::Log, schema_definition).with_port(LOGS),
            SourceOutput::new_metrics().with_port(METRICS),
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
