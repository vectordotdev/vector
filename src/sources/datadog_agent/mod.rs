#[cfg(all(test, feature = "datadog-agent-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

pub mod logs;
pub mod metrics;
pub mod traces;

#[allow(warnings, clippy::pedantic, clippy::nursery)]
pub(crate) mod ddmetric_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

#[allow(warnings)]
pub(crate) mod ddtrace_proto {
    include!(concat!(env!("OUT_DIR"), "/dd_trace.rs"));
}

use std::{convert::Infallible, fmt::Debug, io::Read, net::SocketAddr, sync::Arc, time::Duration};

use bytes::{Buf, Bytes};
use chrono::{DateTime, Utc, serde::ts_milliseconds};
use flate2::read::{MultiGzDecoder, ZlibDecoder};
use futures::FutureExt;
use http::StatusCode;
use hyper::{Server, service::make_service_fn};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use snafu::Snafu;
use tokio::net::TcpStream;
use tower::ServiceBuilder;
use tracing::Span;
use vector_lib::{
    codecs::decoding::{DeserializerConfig, FramingConfig},
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    event::{BatchNotifier, BatchStatus},
    internal_event::{EventsReceived, Registered},
    lookup::owned_value_path,
    schema::meaning,
    source_sender::SendError,
    tls::MaybeTlsIncomingStream,
};
use vrl::{
    path::OwnedTargetPath,
    value::{Kind, kind::Collection},
};
use warp::{Filter, Reply, filters::BoxedFilter, reject::Rejection, reply::Response};

use crate::{
    SourceSender,
    codecs::{Decoder, DecodingConfig},
    common::http::ErrorMessage,
    config::{
        DataType, GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig,
        SourceContext, SourceOutput, log_schema,
    },
    event::Event,
    http::{KeepaliveConfig, MaxConnectionAgeLayer, build_http_trace_layer},
    internal_events::{HttpBytesReceived, StreamClosedError},
    schema,
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::{self, util::http::emit_decompress_error},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

pub const LOGS: &str = "logs";
pub const METRICS: &str = "metrics";
pub const TRACES: &str = "traces";

/// Configuration for the `datadog_agent` source.
#[configurable_component(source(
    "datadog_agent",
    "Receive logs, metrics, and traces collected by a Datadog Agent."
))]
#[serde_as]
#[derive(Clone, Debug)]
pub struct DatadogAgentConfig {
    /// The socket address to accept connections on.
    ///
    /// It _must_ include a port.
    #[configurable(metadata(docs::examples = "0.0.0.0:80"))]
    #[configurable(metadata(docs::examples = "localhost:80"))]
    address: SocketAddr,

    /// If this is set to `true`, when incoming events contain a Datadog API key, it is
    /// stored in the event metadata and used if the event is sent to a Datadog sink.
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "crate::serde::default_true")]
    store_api_key: bool,

    /// If this is set to `true`, logs are not accepted by the component.
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "crate::serde::default_false")]
    disable_logs: bool,

    /// If this is set to `true`, metrics (beta) are not accepted by the component.
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "crate::serde::default_false")]
    disable_metrics: bool,

    /// If this is set to `true`, traces (alpha) are not accepted by the component.
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "crate::serde::default_false")]
    disable_traces: bool,

    /// If this is set to `true`, logs, metrics (beta), and traces (alpha) are sent to different outputs.
    ///
    ///
    /// For a source component named `agent`, the received logs, metrics (beta), and traces (alpha) can then be
    /// configured as input to other components by specifying `agent.logs`, `agent.metrics`, and
    /// `agent.traces`, respectively.
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "crate::serde::default_false")]
    multiple_outputs: bool,

    /// If this is set to `true`, when log events contain the field `ddtags`, the string value that
    /// contains a list of key:value pairs set by the Agent is parsed and expanded into an array.
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "crate::serde::default_false")]
    parse_ddtags: bool,

    /// If this is set to `true`, metric names are split at the first '.' into a namespace and name.
    /// For example, `system.cpu.usage` would be split into namespace `system` and name `cpu.usage`.
    /// If `false`, the full metric name is used without splitting. This may be useful if you are using a
    /// default namespace for metrics in sinks connected to this source.
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "crate::serde::default_true")]
    split_metric_namespace: bool,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    log_namespace: Option<bool>,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    decoding: DeserializerConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    keepalive: KeepaliveConfig,

    /// The timeout before responding to requests with a HTTP 503 Service Unavailable error.
    ///
    /// If not set, responses to completed requests will block indefinitely until connected
    /// transforms or sinks are ready to receive the events. When this happens, the sending Datadog
    /// Agent will eventually time out the request and drop the connection, resulting Vector
    /// generating an "Events dropped." error and incrementing the `component_discarded_events_total`
    /// internal metric. By setting this option to a value less than the Agent's timeout, Vector
    /// will instead respond to the Agent with a HTTP 503 Service Unavailable error, emit a warning,
    /// and increment the `component_timed_out_events_total` internal metric instead.
    #[serde_as(as = "Option<serde_with::DurationSecondsWithFrac<f64>>")]
    send_timeout_secs: Option<f64>,
}

impl GenerateConfig for DatadogAgentConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:8080".parse().unwrap(),
            tls: None,
            store_api_key: true,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            acknowledgements: SourceAcknowledgementsConfig::default(),
            disable_logs: false,
            disable_metrics: false,
            disable_traces: false,
            multiple_outputs: false,
            parse_ddtags: false,
            split_metric_namespace: true,
            log_namespace: Some(false),
            keepalive: KeepaliveConfig::default(),
            send_timeout_secs: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_agent")]
impl SourceConfig for DatadogAgentConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let logs_schema_definition = cx
            .schema_definitions
            .get(&Some(LOGS.to_owned()))
            .or_else(|| cx.schema_definitions.get(&None))
            .cloned();

        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let tls = MaybeTlsSettings::from_config(self.tls.as_ref(), true)?;
        let source = DatadogAgentSource::new(
            self.store_api_key,
            decoder,
            tls.http_protocol_name(),
            logs_schema_definition,
            log_namespace,
            self.parse_ddtags,
            self.split_metric_namespace,
        );
        let listener = tls.bind(&self.address).await?;
        let handler = RequestHandler {
            acknowledgements: cx.do_acknowledgements(self.acknowledgements),
            multiple_outputs: self.multiple_outputs,
            out: cx.out,
        };
        let filters = source.build_warp_filters(handler, self)?;
        let shutdown = cx.shutdown;
        let keepalive_settings = self.keepalive.clone();

        info!(message = "Building HTTP server.", address = %self.address);

        Ok(Box::pin(async move {
            let routes = filters.recover(|r: Rejection| async move {
                if let Some(e_msg) = r.find::<ErrorMessage>() {
                    let json = warp::reply::json(e_msg);
                    Ok(warp::reply::with_status(json, e_msg.status_code()))
                } else {
                    // other internal error - will return 500 internal server error
                    Err(r)
                }
            });

            let span = Span::current();
            let make_svc = make_service_fn(move |conn: &MaybeTlsIncomingStream<TcpStream>| {
                let svc = ServiceBuilder::new()
                    .layer(build_http_trace_layer(span.clone()))
                    .option_layer(keepalive_settings.max_connection_age_secs.map(|secs| {
                        MaxConnectionAgeLayer::new(
                            Duration::from_secs(secs),
                            keepalive_settings.max_connection_age_jitter_factor,
                            conn.peer_addr(),
                        )
                    }))
                    .service(warp::service(routes.clone()));
                futures_util::future::ok::<_, Infallible>(svc)
            });

            Server::builder(hyper::server::accept::from_stream(listener.accept_stream()))
                .serve(make_svc)
                .with_graceful_shutdown(shutdown.map(|_| ()))
                .await
                .map_err(|err| {
                    error!("An error occurred: {:?}.", err);
                })?;

            Ok(())
        }))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let definition = self
            .decoding
            .schema_definition(global_log_namespace.merge(self.log_namespace))
            // NOTE: "status" is intentionally semantically mapped to "severity",
            //       since that is what DD designates as the semantic meaning of status
            // https://docs.datadoghq.com/logs/log_configuration/attributes_naming_convention/?s=severity#reserved-attributes
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("status"))),
                &owned_value_path!("status"),
                Kind::bytes(),
                Some(meaning::SEVERITY),
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("timestamp"))),
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some(meaning::TIMESTAMP),
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("hostname"))),
                &owned_value_path!("hostname"),
                Kind::bytes(),
                Some(meaning::HOST),
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("service"))),
                &owned_value_path!("service"),
                Kind::bytes(),
                Some(meaning::SERVICE),
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("ddsource"))),
                &owned_value_path!("ddsource"),
                Kind::bytes(),
                Some(meaning::SOURCE),
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("ddtags"))),
                &owned_value_path!("ddtags"),
                if self.parse_ddtags {
                    Kind::array(Collection::empty().with_unknown(Kind::bytes())).or_undefined()
                } else {
                    Kind::bytes()
                },
                Some(meaning::TAGS),
            )
            .with_standard_vector_source_metadata();

        let mut output = Vec::with_capacity(1);

        if self.multiple_outputs {
            if !self.disable_logs {
                output.push(SourceOutput::new_maybe_logs(DataType::Log, definition).with_port(LOGS))
            }
            if !self.disable_metrics {
                output.push(SourceOutput::new_metrics().with_port(METRICS))
            }
            if !self.disable_traces {
                output.push(SourceOutput::new_traces().with_port(TRACES))
            }
        } else {
            output.push(SourceOutput::new_maybe_logs(
                DataType::all_bits(),
                definition,
            ))
        }
        output
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }

    fn send_timeout(&self) -> Option<Duration> {
        self.send_timeout_secs.map(Duration::from_secs_f64)
    }
}

#[derive(Clone, Copy, Debug, Snafu)]
pub(crate) enum ApiError {
    ServerShutdown,
}

impl warp::reject::Reject for ApiError {}

#[derive(Deserialize)]
pub struct ApiKeyQueryParams {
    #[serde(rename = "dd-api-key")]
    pub dd_api_key: Option<String>,
}

#[derive(Clone)]
pub(crate) struct DatadogAgentSource {
    pub(crate) api_key_extractor: ApiKeyExtractor,
    pub(crate) log_schema_host_key: OwnedTargetPath,
    pub(crate) log_schema_source_type_key: OwnedTargetPath,
    pub(crate) log_namespace: LogNamespace,
    pub(crate) decoder: Decoder,
    protocol: &'static str,
    logs_schema_definition: Option<Arc<schema::Definition>>,
    events_received: Registered<EventsReceived>,
    parse_ddtags: bool,
    split_metric_namespace: bool,
}

#[derive(Clone)]
pub struct ApiKeyExtractor {
    matcher: Regex,
    store_api_key: bool,
}

impl ApiKeyExtractor {
    pub fn extract(
        &self,
        path: &str,
        header: Option<String>,
        query_params: Option<String>,
    ) -> Option<Arc<str>> {
        if !self.store_api_key {
            return None;
        }
        // Grab from URL first
        self.matcher
            .captures(path)
            .and_then(|cap| cap.name("api_key").map(|key| key.as_str()).map(Arc::from))
            // Try from query params
            .or_else(|| query_params.map(Arc::from))
            // Try from header next
            .or_else(|| header.map(Arc::from))
    }
}

impl DatadogAgentSource {
    pub(crate) fn new(
        store_api_key: bool,
        decoder: Decoder,
        protocol: &'static str,
        logs_schema_definition: Option<schema::Definition>,
        log_namespace: LogNamespace,
        parse_ddtags: bool,
        split_metric_namespace: bool,
    ) -> Self {
        Self {
            api_key_extractor: ApiKeyExtractor {
                store_api_key,
                matcher: Regex::new(r"^/v1/input/(?P<api_key>[[:alnum:]]{32})/??")
                    .expect("static regex always compiles"),
            },
            log_schema_host_key: log_schema()
                .host_key_target_path()
                .expect("global log_schema.host_key to be valid path")
                .clone(),
            log_schema_source_type_key: log_schema()
                .source_type_key_target_path()
                .expect("global log_schema.source_type_key to be valid path")
                .clone(),
            decoder,
            protocol,
            logs_schema_definition: logs_schema_definition.map(Arc::new),
            log_namespace,
            events_received: register!(EventsReceived),
            parse_ddtags,
            split_metric_namespace,
        }
    }

    fn build_warp_filters(
        &self,
        handler: RequestHandler,
        config: &DatadogAgentConfig,
    ) -> crate::Result<BoxedFilter<(Response,)>> {
        let mut filters =
            (!config.disable_logs).then(|| logs::build_warp_filter(handler.clone(), self.clone()));

        if !config.disable_traces {
            let trace_filter = traces::build_warp_filter(handler.clone(), self.clone());
            filters = filters
                .map(|f| f.or(trace_filter.clone()).unify().boxed())
                .or(Some(trace_filter));
        }

        if !config.disable_metrics {
            let metrics_filter = metrics::build_warp_filter(handler, self.clone());
            filters = filters
                .map(|f| f.or(metrics_filter.clone()).unify().boxed())
                .or(Some(metrics_filter));
        }

        filters.ok_or_else(|| "At least one of the supported data type shall be enabled".into())
    }

    pub(crate) fn decode(
        &self,
        header: &Option<String>,
        mut body: Bytes,
        path: &str,
    ) -> Result<Bytes, ErrorMessage> {
        if let Some(encodings) = header {
            for encoding in encodings.rsplit(',').map(str::trim) {
                body = match encoding {
                    "identity" => body,
                    "gzip" | "x-gzip" => {
                        let mut decoded = Vec::new();
                        MultiGzDecoder::new(body.reader())
                            .read_to_end(&mut decoded)
                            .map_err(|error| emit_decompress_error(encoding, error))?;
                        decoded.into()
                    }
                    "zstd" => {
                        let mut decoded = Vec::new();
                        zstd::stream::copy_decode(body.reader(), &mut decoded)
                            .map_err(|error| emit_decompress_error(encoding, error))?;
                        decoded.into()
                    }
                    "deflate" | "x-deflate" => {
                        let mut decoded = Vec::new();
                        ZlibDecoder::new(body.reader())
                            .read_to_end(&mut decoded)
                            .map_err(|error| emit_decompress_error(encoding, error))?;
                        decoded.into()
                    }
                    encoding => {
                        return Err(ErrorMessage::new(
                            StatusCode::UNSUPPORTED_MEDIA_TYPE,
                            format!("Unsupported encoding {encoding}"),
                        ));
                    }
                }
            }
        }
        emit!(HttpBytesReceived {
            byte_size: body.len(),
            http_path: path,
            protocol: self.protocol,
        });
        Ok(body)
    }
}

#[derive(Clone)]
struct RequestHandler {
    acknowledgements: bool,
    multiple_outputs: bool,
    out: SourceSender,
}

impl RequestHandler {
    async fn handle_request(
        mut self,
        events: Result<Vec<Event>, ErrorMessage>,
        output: &'static str,
    ) -> Result<Response, Rejection> {
        match events {
            Ok(events) => self.handle_events(events, output).await,
            Err(err) => Err(warp::reject::custom(err)),
        }
    }

    async fn handle_events(
        &mut self,
        mut events: Vec<Event>,
        output: &'static str,
    ) -> Result<Response, Rejection> {
        let receiver = BatchNotifier::maybe_apply_to(self.acknowledgements, &mut events);
        let count = events.len();
        let output = self.multiple_outputs.then_some(output);

        let result = if let Some(name) = output {
            self.out.send_batch_named(name, events).await
        } else {
            self.out.send_batch(events).await
        };
        match result {
            Ok(()) => {}
            Err(SendError::Closed) => {
                emit!(StreamClosedError { count });
                return Err(warp::reject::custom(ApiError::ServerShutdown));
            }
            Err(SendError::Timeout) => {
                return Ok(warp::reply::with_status(
                    "Service unavailable",
                    StatusCode::SERVICE_UNAVAILABLE,
                )
                .into_response());
            }
        }
        match receiver {
            None => Ok(warp::reply().into_response()),
            Some(receiver) => match receiver.await {
                BatchStatus::Delivered => Ok(warp::reply().into_response()),
                BatchStatus::Errored => Err(warp::reject::custom(ErrorMessage::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Error delivering contents to sink".into(),
                ))),
                BatchStatus::Rejected => Err(warp::reject::custom(ErrorMessage::new(
                    StatusCode::BAD_REQUEST,
                    "Contents failed to deliver to sink".into(),
                ))),
            },
        }
    }
}

// https://github.com/DataDog/datadog-agent/blob/a33248c2bc125920a9577af1e16f12298875a4ad/pkg/logs/processor/json.go#L23-L49
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LogMsg {
    pub message: Bytes,
    pub status: Bytes,
    #[serde(
        deserialize_with = "ts_milliseconds::deserialize",
        serialize_with = "ts_milliseconds::serialize"
    )]
    pub timestamp: DateTime<Utc>,
    pub hostname: Bytes,
    pub service: Bytes,
    pub ddsource: Bytes,
    pub ddtags: Bytes,
}
