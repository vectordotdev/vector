use std::{
    fmt,
    num::NonZeroUsize,
    task::{Context, Poll},
};

use async_trait::async_trait;
use bytes::BytesMut;
use futures::{StreamExt, TryFutureExt, future::BoxFuture, stream::BoxStream};
use http::Uri;
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use prost::Message;
use snafu::Snafu;
use tokio_util::codec::Encoder;
use tonic::body::BoxBody;
use tower::{Service, ServiceBuilder};
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
    codecs::encoding::format::OtlpSerializer,
    config::telemetry,
    configurable::configurable_component,
    internal_event::{ComponentEventsDropped, UNINTENTIONAL},
    opentelemetry::proto::{
        RESOURCE_LOGS_JSON_FIELD, RESOURCE_METRICS_JSON_FIELD, RESOURCE_SPANS_JSON_FIELD,
        collector::{
            logs::v1::{ExportLogsServiceRequest, logs_service_client::LogsServiceClient},
            metrics::v1::{
                ExportMetricsServiceRequest, metrics_service_client::MetricsServiceClient,
            },
            trace::v1::{ExportTraceServiceRequest, trace_service_client::TraceServiceClient},
        },
    },
    partition::Partitioner,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::{BatcherSettings, DriverResponse},
};

use crate::{
    config::{AcknowledgementsConfig, SinkContext, SinkHealthcheckOptions},
    event::{Event, EventFinalizers, EventStatus, Finalizable},
    http::build_proxy_connector,
    internal_events::{EndpointBytesSent, SinkRequestBuildError},
    sinks::util::grpc::{HyperGrpcService, with_default_scheme},
    sinks::{
        Healthcheck, VectorSink,
        util::{
            BatchConfig, RealtimeEventBasedDefaultBatchSettings, ServiceBuilderExt, SinkBuilderExt,
            StreamSink, http::RequestConfig, metadata::RequestMetadataBuilder, retries::RetryLogic,
        },
    },
    template::Template,
    tls::{MaybeTlsSettings, TlsConfig},
};

/// Compression codec for gRPC transport.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GrpcCompression {
    /// No compression.
    #[default]
    None,

    /// [Gzip][gzip] compression.
    ///
    /// [gzip]: https://www.gzip.org/
    Gzip,
}

/// Configuration for the OpenTelemetry sink's gRPC transport.
#[configurable_component]
#[derive(Clone, Debug)]
pub(super) struct GrpcSinkConfig {
    /// The URI to send gRPC requests to.
    ///
    /// The URI _must_ include a port. If the scheme is omitted, `http` is used unless
    /// TLS options are configured, in which case `https` is used.
    ///
    /// # Examples
    ///
    /// - `http://localhost:4317`
    /// - `https://otelcol.example.com:4317`
    #[configurable(metadata(docs::examples = "http://localhost:4317"))]
    #[configurable(metadata(
        docs::warnings = "When using template syntax, the rendered URI is taken from event data. Only use dynamic URIs with trusted event sources to avoid directing Vector to unintended internal network destinations."
    ))]
    pub uri: Template,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: GrpcCompression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeEventBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: RequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GrpcSinkConfig {
    pub(super) async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        // For static URIs, parse at build time for the healthcheck.
        // Dynamic URIs are rendered per-event during sink execution.
        let static_uri = if self.uri.is_dynamic() {
            None
        } else {
            Some(with_default_scheme(
                self.uri
                    .get_ref()
                    .parse()
                    .map_err(|e| format!("invalid URI for gRPC sink: {e}"))?,
                self.tls.is_some(),
            )?)
        };

        // The connector is always TLS-capable so it can handle both http:// and https://
        // URIs per-request, matching the behaviour of the HTTP sink. Whether the scheme
        // defaults to http or https when the URI has no explicit scheme is controlled by
        // whether a `tls:` block is configured.
        let tls_configured = self.tls.is_some();
        let tls = MaybeTlsSettings::tls_client(self.tls.as_ref())?;

        let use_gzip = self.compression == GrpcCompression::Gzip;

        // Split headers into static (literal values) and dynamic (template values).
        // Static headers are pre-parsed once and used for the healthcheck and every export.
        // Dynamic headers are rendered per-event so that templated fields (e.g. tenant IDs)
        // resolve correctly at export time.
        let (static_header_strings, dynamic_header_templates_raw) = self.request.split_headers();

        // Static headers are validated at build time and any invalid key or value is a hard
        // error. Silently dropping them would cause every request to be sent without the
        // intended metadata (e.g. auth headers), which is worse than failing fast.
        let mut static_grpc_headers: Vec<(
            tonic::metadata::AsciiMetadataKey,
            tonic::metadata::AsciiMetadataValue,
        )> = Vec::with_capacity(static_header_strings.len());
        for (k, v) in &static_header_strings {
            let k_lower = k.to_lowercase();
            let key = tonic::metadata::AsciiMetadataKey::from_bytes(k_lower.as_bytes())
                .map_err(|e| format!("invalid gRPC metadata key {k:?}: {e}"))?;
            let value = tonic::metadata::AsciiMetadataValue::try_from(v.as_str())
                .map_err(|e| format!("invalid gRPC metadata value for key {k:?}: {e}"))?;
            static_grpc_headers.push((key, value));
        }

        // Dynamic header key names are known at build time and validated eagerly.
        // Values are templated and validated per-event at runtime.
        let mut dynamic_grpc_header_templates: Vec<(tonic::metadata::AsciiMetadataKey, Template)> =
            Vec::with_capacity(dynamic_header_templates_raw.len());
        for (k, t) in dynamic_header_templates_raw {
            let k_lower = k.to_lowercase();
            let key = tonic::metadata::AsciiMetadataKey::from_bytes(k_lower.as_bytes())
                .map_err(|e| format!("invalid gRPC metadata key {k:?}: {e}"))?;
            dynamic_grpc_header_templates.push((key, t));
        }

        let client = new_grpc_client(&tls, cx.proxy())?;
        // Dynamic headers cannot be rendered without a live event, so the healthcheck cannot
        // include them. Rather than running a check that omits required auth metadata (which
        // would cause a false failure against a properly secured collector), skip the
        // healthcheck entirely when dynamic header templates are present — unless the operator
        // has supplied an explicit `healthcheck.uri` override, in which case we honour that URI
        // with only the static headers (the override endpoint does not require the dynamic auth).
        let explicit_healthcheck_uri = cx
            .healthcheck
            .uri
            .clone()
            .map(|u| with_default_scheme(u.uri, tls_configured))
            .transpose()?;
        let healthcheck_uri = if !dynamic_grpc_header_templates.is_empty()
            && explicit_healthcheck_uri.is_none()
        {
            warn!(
                "Skipping gRPC healthcheck: dynamic (templated) headers are configured and \
                 cannot be rendered at startup. Set a static `healthcheck.uri` override to \
                 re-enable the healthcheck."
            );
            None
        } else {
            explicit_healthcheck_uri.or(static_uri.clone())
        };
        let healthcheck = Box::pin(grpc_healthcheck(
            client.clone(),
            healthcheck_uri,
            static_grpc_headers.clone(),
            cx.healthcheck,
        ));
        let service = OtlpGrpcService::new(client, use_gzip, static_grpc_headers);

        let request_settings = self.request.tower.into_settings();
        let batch_settings = self.batch.into_batcher_settings()?;

        let service = ServiceBuilder::new()
            .settings(request_settings, OtlpGrpcRetryLogic)
            .service(service);

        let sink = OtlpGrpcSink {
            batch_settings,
            service,
            dynamic_header_templates: dynamic_grpc_header_templates,
            uri_template: self.uri.clone(),
            tls_configured,
        };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

fn new_grpc_client(
    tls_settings: &MaybeTlsSettings,
    proxy_config: &crate::config::ProxyConfig,
) -> crate::Result<hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>> {
    let proxy = build_proxy_connector(tls_settings.clone(), proxy_config)?;
    Ok(hyper::Client::builder().http2_only(true).build(proxy))
}

async fn grpc_healthcheck(
    client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
    uri: Option<Uri>,
    headers: Vec<(
        tonic::metadata::AsciiMetadataKey,
        tonic::metadata::AsciiMetadataValue,
    )>,
    options: SinkHealthcheckOptions,
) -> crate::Result<()> {
    if !options.enabled {
        return Ok(());
    }

    let Some(uri) = uri else {
        warn!(
            "Skipping gRPC healthcheck: `uri` is a dynamic template and cannot be validated \
             at startup. To enable healthchecking, use a static URI."
        );
        return Ok(());
    };

    use tonic::Code;
    use tonic_health::pb::{HealthCheckRequest, health_client::HealthClient};

    let svc = HyperSvc::new(uri, client);
    let mut health_client = HealthClient::new(svc);

    let mut req = tonic::Request::new(HealthCheckRequest {
        service: String::new(),
    });
    for (key, value) in headers {
        req.metadata_mut().insert(key, value);
    }

    match health_client.check(req).await {
        Ok(response) => {
            use tonic_health::pb::health_check_response::ServingStatus;
            let status = response.into_inner().status;
            if status == ServingStatus::Serving as i32 {
                Ok(())
            } else {
                Err(format!("gRPC collector reported non-serving status: {status}").into())
            }
        }
        // Server is reachable but does not implement the health protocol; treat as healthy.
        Err(status) if status.code() == Code::Unimplemented => Ok(()),
        Err(status) => Err(Box::new(OtlpGrpcError::Request { source: status })),
    }
}

// ── Retry logic ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct OtlpGrpcRetryLogic;

impl RetryLogic for OtlpGrpcRetryLogic {
    type Error = OtlpGrpcError;
    type Request = OtlpGrpcRequest;
    type Response = OtlpGrpcResponse;

    fn is_retriable_error(&self, err: &Self::Error) -> bool {
        use tonic::Code::*;

        match err {
            OtlpGrpcError::Request { source } => !matches!(
                source.code(),
                // List taken from
                // <https://github.com/grpc/grpc/blob/ed1b20777c69bd47e730a63271eafc1b299f6ca0/doc/statuscodes.md>
                NotFound
                    | InvalidArgument
                    | AlreadyExists
                    | PermissionDenied
                    | OutOfRange
                    | Unimplemented
                    | Unauthenticated
                    // DataLoss: per gRPC spec this means unrecoverable data corruption, not
                    // retriable. Note that the OTLP partial-success model can also surface
                    // DataLoss for a partial write; we treat it as non-retriable (consistent with
                    // the vector sink) to avoid sending an already-accepted partial batch twice.
                    | DataLoss
            ),
        }
    }
}

// ── Error type ───────────────────────────────────────────────────────────────

#[derive(Debug, Snafu)]
enum OtlpGrpcError {
    #[snafu(display("gRPC request failed: {source}"))]
    Request { source: tonic::Status },
}

// ── Request/Response ─────────────────────────────────────────────────────────

/// A single-signal OTLP export request. One request per signal type ensures
/// that retries are atomic: a failed metrics export cannot duplicate a
/// previously-accepted logs export.
#[derive(Clone)]
struct OtlpGrpcRequest {
    signal: OtlpSignal,
    uri: Uri,
    /// Per-event rendered values for dynamic (templated) metadata headers.
    dynamic_headers: Vec<(
        tonic::metadata::AsciiMetadataKey,
        tonic::metadata::AsciiMetadataValue,
    )>,
    finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl Finalizable for OtlpGrpcRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for OtlpGrpcRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

struct OtlpGrpcResponse {
    events_byte_size: GroupedCountByteSize,
    /// True when the collector returned a partial-success response with one or more rejected
    /// records. The entire batch is marked `Rejected` so that upstream sources are notified of
    /// data loss. This may cause the accepted portion to be re-sent if the source retries, but
    /// silent data loss is worse than potential duplication.
    had_partial_success: bool,
}

impl DriverResponse for OtlpGrpcResponse {
    fn event_status(&self) -> EventStatus {
        if self.had_partial_success {
            EventStatus::Rejected
        } else {
            EventStatus::Delivered
        }
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }
}

// ── Service ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct CachedClients {
    uri: Uri,
    logs: LogsServiceClient<HyperSvc>,
    metrics: MetricsServiceClient<HyperSvc>,
    traces: TraceServiceClient<HyperSvc>,
}

/// Holds whichever tonic client is needed for a single `OtlpGrpcRequest`. Only the
/// relevant client is cloned out of `CachedClients`, avoiding two unnecessary clones
/// per `Service::call` invocation.
enum SignalClient {
    Logs(LogsServiceClient<HyperSvc>),
    Metrics(MetricsServiceClient<HyperSvc>),
    Traces(TraceServiceClient<HyperSvc>),
}

#[derive(Clone)]
struct OtlpGrpcService {
    /// Tonic clients for the most-recently-seen URI; rebuilt when the rendered URI changes.
    /// Each Tower concurrency-slot clone owns its cache independently — no shared mutex needed.
    clients: Option<CachedClients>,
    hyper_client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
    compression: bool,
    headers: std::sync::Arc<
        Vec<(
            tonic::metadata::AsciiMetadataKey,
            tonic::metadata::AsciiMetadataValue,
        )>,
    >,
}

impl OtlpGrpcService {
    fn new(
        hyper_client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
        compression: bool,
        headers: Vec<(
            tonic::metadata::AsciiMetadataKey,
            tonic::metadata::AsciiMetadataValue,
        )>,
    ) -> Self {
        Self {
            clients: None,
            hyper_client,
            compression,
            headers: std::sync::Arc::new(headers),
        }
    }

    /// Rebuilds cached tonic clients if the URI changed, then returns the single client
    /// matching `signal`. Only that one client is cloned.
    fn client_for_signal(&mut self, uri: &Uri, signal: &OtlpSignal) -> SignalClient {
        if self.clients.as_ref().is_none_or(|c| &c.uri != uri) {
            let svc = HyperSvc::new(uri.clone(), self.hyper_client.clone());
            let mut logs = LogsServiceClient::new(svc.clone());
            let mut metrics = MetricsServiceClient::new(svc.clone());
            let mut traces = TraceServiceClient::new(svc);
            if self.compression {
                logs = logs.send_compressed(tonic::codec::CompressionEncoding::Gzip);
                metrics = metrics.send_compressed(tonic::codec::CompressionEncoding::Gzip);
                traces = traces.send_compressed(tonic::codec::CompressionEncoding::Gzip);
            }
            self.clients = Some(CachedClients {
                uri: uri.clone(),
                logs,
                metrics,
                traces,
            });
        }
        let c = self.clients.as_ref().expect("just populated");
        match signal {
            OtlpSignal::Logs(_) => SignalClient::Logs(c.logs.clone()),
            OtlpSignal::Metrics(_) => SignalClient::Metrics(c.metrics.clone()),
            OtlpSignal::Traces(_) => SignalClient::Traces(c.traces.clone()),
        }
    }
}

impl Service<OtlpGrpcRequest> for OtlpGrpcService {
    type Response = OtlpGrpcResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: OtlpGrpcRequest) -> Self::Future {
        let (protocol, endpoint) = crate::sinks::util::uri::protocol_endpoint(req.uri.clone());
        // Rebuild clients if the URI changed; clone only the client for this signal type.
        let client = self.client_for_signal(&req.uri, &req.signal);
        let static_headers = std::sync::Arc::clone(&self.headers);
        let metadata = std::mem::take(req.metadata_mut());
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();

        let future = async move {
            macro_rules! export {
                ($client:expr, $payload:expr) => {{
                    let len = $payload.encoded_len();
                    let mut grpc_req = tonic::Request::new($payload);
                    for (key, value) in static_headers.iter() {
                        grpc_req.metadata_mut().insert(key.clone(), value.clone());
                    }
                    for (key, value) in &req.dynamic_headers {
                        grpc_req.metadata_mut().insert(key.clone(), value.clone());
                    }
                    let response = $client
                        .export(grpc_req)
                        .map_err(|source| OtlpGrpcError::Request { source })
                        .await?;
                    (len, response.into_inner())
                }};
            }

            // The signal variant and the client type are always aligned — both are derived
            // from `req.signal` in `client_for_signal` — so the mixed arms are unreachable.
            let (byte_size, had_partial_success) = match (req.signal, client) {
                (OtlpSignal::Logs(r), SignalClient::Logs(mut c)) => {
                    let (len, resp) = export!(c, r);
                    let partial = if let Some(ps) = resp.partial_success
                        && ps.rejected_log_records > 0
                    {
                        warn!(
                            rejected = ps.rejected_log_records,
                            message = ps.error_message,
                            "OTLP collector rejected log records"
                        );
                        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                            count: ps.rejected_log_records as usize,
                            reason: if ps.error_message.is_empty() {
                                "OTLP partial success rejection"
                            } else {
                                &ps.error_message
                            },
                        });
                        true
                    } else {
                        false
                    };
                    (len, partial)
                }
                (OtlpSignal::Metrics(r), SignalClient::Metrics(mut c)) => {
                    let (len, resp) = export!(c, r);
                    let partial = if let Some(ps) = resp.partial_success
                        && ps.rejected_data_points > 0
                    {
                        warn!(
                            rejected = ps.rejected_data_points,
                            message = ps.error_message,
                            "OTLP collector rejected metric data points"
                        );
                        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                            count: ps.rejected_data_points as usize,
                            reason: if ps.error_message.is_empty() {
                                "OTLP partial success rejection"
                            } else {
                                &ps.error_message
                            },
                        });
                        true
                    } else {
                        false
                    };
                    (len, partial)
                }
                (OtlpSignal::Traces(r), SignalClient::Traces(mut c)) => {
                    let (len, resp) = export!(c, r);
                    let partial = if let Some(ps) = resp.partial_success
                        && ps.rejected_spans > 0
                    {
                        warn!(
                            rejected = ps.rejected_spans,
                            message = ps.error_message,
                            "OTLP collector rejected trace spans"
                        );
                        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                            count: ps.rejected_spans as usize,
                            reason: if ps.error_message.is_empty() {
                                "OTLP partial success rejection"
                            } else {
                                &ps.error_message
                            },
                        });
                        true
                    } else {
                        false
                    };
                    (len, partial)
                }
                _ => unreachable!("signal variant and cached client are always aligned"),
            };

            emit!(EndpointBytesSent {
                byte_size,
                protocol: &protocol,
                endpoint: &endpoint,
            });

            Ok(OtlpGrpcResponse {
                events_byte_size,
                had_partial_success,
            })
        };

        Box::pin(future.map_err(|err: OtlpGrpcError| -> crate::Error { Box::new(err) }))
    }
}

type HyperSvc = HyperGrpcService;

// ── Sink ─────────────────────────────────────────────────────────────────────

/// Partition key for gRPC batches. Events that render to different URIs or
/// dynamic headers must be sent in separate requests, so they are batched
/// independently. The key stores rendered values as strings so it can derive
/// `Hash + Eq` without requiring those traits from `Uri` or tonic types.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct BatchPartitionKey {
    uri: String,
    /// Dynamic header (key, rendered-value) pairs, sorted for deterministic equality.
    headers: Vec<(String, String)>,
}

struct GrpcPartitioner;

impl Partitioner for GrpcPartitioner {
    type Item = OtlpEventData;
    type Key = BatchPartitionKey;

    fn partition(&self, item: &OtlpEventData) -> BatchPartitionKey {
        let mut headers: Vec<(String, String)> = item
            .dynamic_headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_owned()))
            .collect();
        headers.sort_unstable();
        BatchPartitionKey {
            uri: item.uri.to_string(),
            headers,
        }
    }
}

/// Intermediate event data extracted before batching.
struct OtlpEventData {
    byte_size: usize,
    json_byte_size: GroupedCountByteSize,
    finalizers: EventFinalizers,
    signal: OtlpSignal,
    uri: Uri,
    dynamic_headers: Vec<(
        tonic::metadata::AsciiMetadataKey,
        tonic::metadata::AsciiMetadataValue,
    )>,
}

impl ByteSizeOf for OtlpEventData {
    fn size_of(&self) -> usize {
        std::mem::size_of::<Self>() + self.allocated_bytes()
    }

    fn allocated_bytes(&self) -> usize {
        self.byte_size
    }
}

/// OTLP signal payload. Used as per-event intermediate data before batching and as the
/// request payload sent via gRPC after batching. Each variant corresponds to one signal
/// type so that requests can be retried independently.
#[derive(Clone)]
enum OtlpSignal {
    Logs(ExportLogsServiceRequest),
    Metrics(ExportMetricsServiceRequest),
    Traces(ExportTraceServiceRequest),
}

/// Per-signal accumulator tracking the merged proto request and its associated
/// event metadata. Kept separate so each signal can be retried independently.
struct SignalData<R> {
    request: R,
    finalizers: EventFinalizers,
    event_count: usize,
    byte_size: usize,
    json_byte_size: GroupedCountByteSize,
}

impl<R> SignalData<R> {
    const fn new(
        request: R,
        finalizers: EventFinalizers,
        byte_size: usize,
        json_byte_size: GroupedCountByteSize,
    ) -> Self {
        SignalData {
            request,
            finalizers,
            event_count: 1,
            byte_size,
            json_byte_size,
        }
    }

    fn merge(
        &mut self,
        req: R,
        finalizers: EventFinalizers,
        byte_size: usize,
        json_byte_size: GroupedCountByteSize,
        merge_req: impl FnOnce(&mut R, R),
    ) {
        merge_req(&mut self.request, req);
        self.finalizers.merge(finalizers);
        self.event_count += 1;
        self.byte_size += byte_size;
        self.json_byte_size += json_byte_size;
    }
}

/// Accumulator for a batch of OTLP events, separated by signal type so that
/// the resulting requests can be retried independently.
#[derive(Default)]
struct OtlpBatch {
    logs: Option<SignalData<ExportLogsServiceRequest>>,
    metrics: Option<SignalData<ExportMetricsServiceRequest>>,
    traces: Option<SignalData<ExportTraceServiceRequest>>,
}

impl OtlpBatch {
    fn push(&mut self, item: OtlpEventData) {
        let OtlpEventData {
            byte_size,
            json_byte_size,
            finalizers,
            signal,
            uri: _,
            dynamic_headers: _,
        } = item;
        match signal {
            OtlpSignal::Logs(req) => match &mut self.logs {
                Some(existing) => {
                    existing.merge(req, finalizers, byte_size, json_byte_size, |e, r| {
                        e.resource_logs.extend(r.resource_logs)
                    })
                }
                slot => *slot = Some(SignalData::new(req, finalizers, byte_size, json_byte_size)),
            },
            OtlpSignal::Metrics(req) => match &mut self.metrics {
                Some(existing) => {
                    existing.merge(req, finalizers, byte_size, json_byte_size, |e, r| {
                        e.resource_metrics.extend(r.resource_metrics)
                    })
                }
                slot => *slot = Some(SignalData::new(req, finalizers, byte_size, json_byte_size)),
            },
            OtlpSignal::Traces(req) => match &mut self.traces {
                Some(existing) => {
                    existing.merge(req, finalizers, byte_size, json_byte_size, |e, r| {
                        e.resource_spans.extend(r.resource_spans)
                    })
                }
                slot => *slot = Some(SignalData::new(req, finalizers, byte_size, json_byte_size)),
            },
        }
    }
}

struct OtlpGrpcSink<S> {
    batch_settings: BatcherSettings,
    service: S,
    uri_template: Template,
    /// Used only to pick the default scheme (`http` vs `https`) when the rendered URI
    /// has no explicit scheme. The connector itself handles both schemes.
    tls_configured: bool,
    dynamic_header_templates: Vec<(tonic::metadata::AsciiMetadataKey, Template)>,
}

impl<S> OtlpGrpcSink<S>
where
    S: Service<OtlpGrpcRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mut serializer = OtlpSerializer::new().map_err(|e| {
            error!("Failed to create OTLP serializer: {}", e);
        })?;

        let uri_template = self.uri_template.clone();
        let tls_configured = self.tls_configured;
        let dynamic_header_templates = self.dynamic_header_templates.clone();

        input
            .filter_map(move |mut event| {
                let rendered = match uri_template.render_string(&event) {
                    Ok(s) => s,
                    Err(e) => return reject_event(event.take_finalizers(), format!("failed to render gRPC URI template: {e}")),
                };
                let parsed = match rendered.parse::<Uri>() {
                    Ok(u) => u,
                    Err(e) => return reject_event(event.take_finalizers(), format!("failed to parse rendered gRPC URI: {e}")),
                };
                let uri = match with_default_scheme(parsed, tls_configured) {
                    Ok(u) => u,
                    Err(e) => return reject_event(event.take_finalizers(), format!("invalid gRPC URI after rendering template: {e}")),
                };

                // Render dynamic headers. If any required header fails to render or produces a
                // non-ASCII value, drop the entire event rather than forwarding without it.
                // Silently omitting a header (e.g. X-Tenant-ID) could bypass authorization on
                // the receiving collector.
                let mut dynamic_headers = Vec::with_capacity(dynamic_header_templates.len());
                for (key, template) in &dynamic_header_templates {
                    match template.render_string(&event) {
                        Ok(rendered) => {
                            match tonic::metadata::AsciiMetadataValue::try_from(rendered.as_str()) {
                                Ok(value) => dynamic_headers.push((key.clone(), value)),
                                Err(e) => {
                                    return reject_event(event.take_finalizers(), format!(
                                        "gRPC metadata value for key {:?} is not valid ASCII: {e}",
                                        key.as_str()
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            return reject_event(event.take_finalizers(), format!(
                                "failed to render gRPC metadata template for key {:?}: {e}",
                                key.as_str()
                            ));
                        }
                    }
                }

                // TODO(perf): OtlpSerializer only exposes a byte-level Encoder interface, so
                // encode_event must encode to bytes and then decode back to a typed proto struct.
                // Exposing a typed output from OtlpSerializer would eliminate this roundtrip.
                let signal = encode_event(&mut serializer, &event);
                match signal {
                    Ok(Some(signal)) => {
                        let mut json_byte_size = telemetry().create_request_count_byte_size();
                        json_byte_size.add_event(&event, event.estimated_json_encoded_size_of());
                        let data = OtlpEventData {
                            byte_size: event.size_of(),
                            json_byte_size,
                            finalizers: event.take_finalizers(),
                            signal,
                            uri,
                            dynamic_headers,
                        };
                        futures::future::ready(Some(data))
                    }
                    Ok(None) => {
                        // Event does not contain OTLP structure (missing resourceLogs,
                        // resourceMetrics, or resourceSpans field). This sink only accepts
                        // OTLP-structured events; plain log events from non-OTLP sources
                        // must be encoded with an OTLP codec before reaching this sink.
                        reject_event(
                            event.take_finalizers(),
                            "event is not OTLP-encoded (missing resourceLogs, resourceMetrics, \
                             or resourceSpans field); this sink only accepts OTLP-structured events",
                        )
                    }
                    Err(e) => reject_event(event.take_finalizers(), e.to_string()),
                }
            })
            // Partition by (rendered URI, dynamic headers) so that events destined for
            // different collectors or with different tenant metadata are never merged into
            // the same batch. Each partition is independently flushed by size or timeout.
            //
            // `as_byte_size_config` is intentional here: the primary flush triggers are the
            // event-count limit (default 1000, from `RealtimeEventBasedDefaultBatchSettings`)
            // and the 1s timeout. The byte limit defaults to `usize::MAX` (effectively
            // disabled) because `MAX_BYTES = None` for that settings type. `as_byte_size_config`
            // selects `ByteSizeOf` as the size metric so that an operator-configured
            // `batch.max_bytes` is measured consistently with the rest of Vector.
            .batched_partitioned(
                GrpcPartitioner,
                self.batch_settings.timeout,
                |_| self.batch_settings.as_byte_size_config(),
            )
            .flat_map(|(_, mut items)| {
                // All items in a partition share the same URI and dynamic headers by construction;
                // take them from the first item rather than re-parsing from the partition key.
                let (uri, dynamic_headers) = match items.first() {
                    Some(first) => (first.uri.clone(), first.dynamic_headers.clone()),
                    None => return futures::stream::iter(Vec::new()),
                };

                // Reduce the partitioned items into per-signal accumulators.
                let mut batch = OtlpBatch::default();
                for item in items.drain(..) {
                    batch.push(item);
                }

                let mut requests = Vec::new();

                if let Some(data) = batch.logs {
                    requests.push(signal_into_request(data, OtlpSignal::Logs, &uri, dynamic_headers.clone()));
                }
                if let Some(data) = batch.metrics {
                    requests.push(signal_into_request(data, OtlpSignal::Metrics, &uri, dynamic_headers.clone()));
                }
                if let Some(data) = batch.traces {
                    requests.push(signal_into_request(data, OtlpSignal::Traces, &uri, dynamic_headers.clone()));
                }

                futures::stream::iter(requests)
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for OtlpGrpcSink<S>
where
    S: Service<OtlpGrpcRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Encode a Vector event into an OTLP proto signal using [`OtlpSerializer`].
///
/// Returns `Ok(None)` for native Vector `Metric` events (not supported by OTLP
/// serializer). Returns `Err` for unexpected encoding failures.
fn encode_event(
    serializer: &mut OtlpSerializer,
    event: &Event,
) -> Result<Option<OtlpSignal>, vector_common::Error> {
    let mut buf = BytesMut::new();
    // Clone the event to pass ownership to the encoder while keeping the original
    // for metadata extraction in the caller. The clone is shallow for logs/traces.
    match event {
        Event::Log(log) if log.contains(RESOURCE_LOGS_JSON_FIELD) => {
            serializer.encode(event.clone(), &mut buf)?;
            Ok(Some(OtlpSignal::Logs(ExportLogsServiceRequest::decode(
                buf.freeze(),
            )?)))
        }
        Event::Log(log) if log.contains(RESOURCE_METRICS_JSON_FIELD) => {
            serializer.encode(event.clone(), &mut buf)?;
            Ok(Some(OtlpSignal::Metrics(
                ExportMetricsServiceRequest::decode(buf.freeze())?,
            )))
        }
        // OTLP spans can arrive as Log events when the source does not use
        // use_otlp_decoding.traces = true.
        Event::Log(log) if log.contains(RESOURCE_SPANS_JSON_FIELD) => {
            serializer.encode(event.clone(), &mut buf)?;
            Ok(Some(OtlpSignal::Traces(ExportTraceServiceRequest::decode(
                buf.freeze(),
            )?)))
        }
        Event::Trace(trace) if trace.contains(RESOURCE_SPANS_JSON_FIELD) => {
            serializer.encode(event.clone(), &mut buf)?;
            Ok(Some(OtlpSignal::Traces(ExportTraceServiceRequest::decode(
                buf.freeze(),
            )?)))
        }
        _ => Ok(None),
    }
}

/// Drop an event that could not be prepared for export.
///
/// Marks `finalizers` as [`EventStatus::Rejected`] so that upstream sources are
/// notified of the failure, then emits [`SinkRequestBuildError`] and
/// [`ComponentEventsDropped`]. Returns a ready future resolving to `None` so
/// the caller can use `return reject_event(...)` directly inside a `filter_map`
/// closure.
fn reject_event(
    finalizers: EventFinalizers,
    reason: impl fmt::Display,
) -> futures::future::Ready<Option<OtlpEventData>> {
    let reason = reason.to_string();
    finalizers.update_status(EventStatus::Rejected);
    emit!(SinkRequestBuildError { error: &reason });
    emit!(ComponentEventsDropped::<UNINTENTIONAL> {
        count: 1,
        reason: &reason,
    });
    futures::future::ready(None)
}

/// Build an [`OtlpGrpcRequest`] from a per-signal accumulator.
fn signal_into_request<R: prost::Message>(
    data: SignalData<R>,
    make_signal: fn(R) -> OtlpSignal,
    uri: &Uri,
    dynamic_headers: Vec<(
        tonic::metadata::AsciiMetadataKey,
        tonic::metadata::AsciiMetadataValue,
    )>,
) -> OtlpGrpcRequest {
    let byte_size = data.request.encoded_len();
    let bytes_len = NonZeroUsize::new(byte_size.max(1)).expect("should be non-zero");
    let builder =
        RequestMetadataBuilder::new(data.event_count, data.byte_size, data.json_byte_size);
    OtlpGrpcRequest {
        signal: make_signal(data.request),
        uri: uri.clone(),
        dynamic_headers,
        finalizers: data.finalizers,
        metadata: builder.with_request_size(bytes_len),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::grpc::with_default_scheme;

    #[test]
    fn generate_grpc_config() {
        let config: GrpcSinkConfig = toml::from_str(
            r#"
            uri = "http://localhost:4317"
        "#,
        )
        .unwrap();
        assert_eq!(config.uri.get_ref(), "http://localhost:4317");
        assert_eq!(config.compression, GrpcCompression::default());
    }

    #[test]
    fn grpc_config_with_gzip() {
        let config: GrpcSinkConfig = toml::from_str(
            r#"
            uri = "https://otelcol.example.com:4317"
            compression = "gzip"
        "#,
        )
        .unwrap();
        assert_eq!(config.uri.get_ref(), "https://otelcol.example.com:4317");
        assert_eq!(config.compression, GrpcCompression::Gzip);
    }

    #[test]
    fn with_default_scheme_adds_http() {
        let uri = with_default_scheme("localhost:4317".parse().unwrap(), false).unwrap();
        assert_eq!(uri.scheme_str(), Some("http"));
    }

    #[test]
    fn with_default_scheme_adds_https() {
        let uri = with_default_scheme("localhost:4317".parse().unwrap(), true).unwrap();
        assert_eq!(uri.scheme_str(), Some("https"));
    }

    #[test]
    fn with_default_scheme_preserves_existing() {
        let uri = with_default_scheme("http://localhost:4317".parse().unwrap(), true).unwrap();
        assert_eq!(uri.scheme_str(), Some("http"));
    }
}
