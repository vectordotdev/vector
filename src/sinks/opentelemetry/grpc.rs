use std::{
    fmt,
    num::NonZeroUsize,
    task::{Context, Poll},
};

use async_trait::async_trait;
use bytes::BytesMut;
use futures::{StreamExt, TryFutureExt, future::BoxFuture, stream::BoxStream};
use http::{
    Uri,
    uri::{PathAndQuery, Scheme},
};
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
    config::{AcknowledgementsConfig, DataType, Input, SinkContext, SinkHealthcheckOptions},
    event::{Event, EventFinalizers, EventStatus, Finalizable},
    http::build_proxy_connector,
    internal_events::EndpointBytesSent,
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

pub(super) fn with_default_scheme(uri: Uri, tls: bool) -> crate::Result<Uri> {
    if uri.scheme().is_none() {
        let mut parts = uri.into_parts();
        parts.scheme = Some(if tls { Scheme::HTTPS } else { Scheme::HTTP });
        if parts.path_and_query.is_none() {
            parts.path_and_query = Some(PathAndQuery::from_static("/"));
        }
        Ok(Uri::from_parts(parts)?)
    } else {
        Ok(uri)
    }
}

/// Configuration for the OpenTelemetry sink's gRPC transport.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct GrpcSinkConfig {
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
    pub async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
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

        // For dynamic templates like `https://{{ host }}:4317` the static_uri is None, so
        // we also check whether the literal prefix of the template string is "https://".
        // This covers the common case where the scheme is a fixed literal even though the
        // host/port are templated.
        let use_https = self.tls.is_some()
            || static_uri
                .as_ref()
                .is_some_and(|u| u.scheme_str() == Some("https"))
            || self.uri.get_ref().to_ascii_lowercase().starts_with("https://");

        let tls = if use_https {
            MaybeTlsSettings::tls_client(self.tls.as_ref())?
        } else {
            MaybeTlsSettings::Raw(())
        };

        let use_gzip = self.compression == GrpcCompression::Gzip;

        // Split headers into static (literal values) and dynamic (template values).
        // Static headers are pre-parsed once and used for the healthcheck and every export.
        // Dynamic headers are rendered per-event so that templated fields (e.g. tenant IDs)
        // resolve correctly at export time.
        let (static_header_strings, dynamic_header_templates_raw) = self.request.split_headers();

        let parse_key = |k: &str| {
            tonic::metadata::AsciiMetadataKey::from_bytes(k.as_bytes())
                .map_err(|e| warn!("Skipping invalid gRPC metadata key {k:?}: {e}"))
                .ok()
        };

        let static_grpc_headers: Vec<(
            tonic::metadata::AsciiMetadataKey,
            tonic::metadata::AsciiMetadataValue,
        )> = static_header_strings
            .iter()
            .filter_map(|(k, v)| {
                let key = parse_key(k)?;
                let value = tonic::metadata::AsciiMetadataValue::try_from(v.as_str())
                    .map_err(|e| warn!("Skipping invalid gRPC metadata value for {k:?}: {e}"))
                    .ok()?;
                Some((key, value))
            })
            .collect();

        let dynamic_grpc_header_templates: Vec<(tonic::metadata::AsciiMetadataKey, Template)> =
            dynamic_header_templates_raw
                .into_iter()
                .filter_map(|(k, t)| Some((parse_key(&k)?, t)))
                .collect();

        let client = new_grpc_client(&tls, cx.proxy())?;
        let healthcheck = Box::pin(grpc_healthcheck(
            client.clone(),
            static_uri,
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
            use_https,
        };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    pub fn input(&self) -> Input {
        // Native Vector Metric events are not supported; OTLP-encoded metrics arrive as Log
        // events with a `resourceMetrics` field and are handled correctly.
        Input::new(DataType::Log | DataType::Trace)
    }

    pub const fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
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

    let svc = HyperSvc { uri, client };
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
#[snafu(visibility(pub))]
pub enum OtlpGrpcError {
    #[snafu(display("gRPC request failed: {source}"))]
    Request { source: tonic::Status },
}

// ── Request/Response ─────────────────────────────────────────────────────────

/// A single-signal OTLP export request. One request per signal type ensures
/// that retries are atomic: a failed metrics export cannot duplicate a
/// previously-accepted logs export.
#[derive(Clone)]
pub struct OtlpGrpcRequest {
    pub signal: OtlpSignal,
    pub uri: Uri,
    /// Per-event rendered values for dynamic (templated) metadata headers.
    pub dynamic_headers: Vec<(
        tonic::metadata::AsciiMetadataKey,
        tonic::metadata::AsciiMetadataValue,
    )>,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
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

pub struct OtlpGrpcResponse {
    events_byte_size: GroupedCountByteSize,
}

impl DriverResponse for OtlpGrpcResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }
}

// ── Service ──────────────────────────────────────────────────────────────────

struct CachedClients {
    uri: Uri,
    logs: LogsServiceClient<HyperSvc>,
    metrics: MetricsServiceClient<HyperSvc>,
    traces: TraceServiceClient<HyperSvc>,
}

#[derive(Clone)]
pub struct OtlpGrpcService {
    /// Tonic clients for the most-recently-seen URI; rebuilt when the rendered URI changes.
    clients: std::sync::Arc<std::sync::Mutex<Option<CachedClients>>>,
    hyper_client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
    compression: bool,
    headers: Vec<(
        tonic::metadata::AsciiMetadataKey,
        tonic::metadata::AsciiMetadataValue,
    )>,
}

impl OtlpGrpcService {
    pub fn new(
        hyper_client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
        compression: bool,
        headers: Vec<(
            tonic::metadata::AsciiMetadataKey,
            tonic::metadata::AsciiMetadataValue,
        )>,
    ) -> Self {
        Self {
            clients: std::sync::Arc::new(std::sync::Mutex::new(None)),
            hyper_client,
            compression,
            headers,
        }
    }

    /// Returns cloned tonic clients for `uri`, rebuilding them if the URI changed.
    /// The mutex is held only during the synchronous rebuild, never across any `.await`.
    fn clients_for(
        &self,
        uri: &Uri,
    ) -> (
        LogsServiceClient<HyperSvc>,
        MetricsServiceClient<HyperSvc>,
        TraceServiceClient<HyperSvc>,
    ) {
        let mut guard = self.clients.lock().expect("client lock poisoned");
        if guard.as_ref().is_none_or(|c| &c.uri != uri) {
            let svc = HyperSvc {
                uri: uri.clone(),
                client: self.hyper_client.clone(),
            };
            let mut logs = LogsServiceClient::new(svc.clone());
            let mut metrics = MetricsServiceClient::new(svc.clone());
            let mut traces = TraceServiceClient::new(svc);
            if self.compression {
                logs = logs.send_compressed(tonic::codec::CompressionEncoding::Gzip);
                metrics = metrics.send_compressed(tonic::codec::CompressionEncoding::Gzip);
                traces = traces.send_compressed(tonic::codec::CompressionEncoding::Gzip);
            }
            *guard = Some(CachedClients {
                uri: uri.clone(),
                logs,
                metrics,
                traces,
            });
        }
        let c = guard.as_ref().expect("just populated");
        (c.logs.clone(), c.metrics.clone(), c.traces.clone())
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
        // Rebuild clients if the URI changed; clone them out before any `.await`.
        let (mut logs_client, mut metrics_client, mut traces_client) = self.clients_for(&req.uri);
        let static_headers = self.headers.clone();
        let metadata = std::mem::take(req.metadata_mut());
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();

        let future = async move {
            macro_rules! export {
                ($client:expr, $payload:expr) => {{
                    let len = $payload.encoded_len();
                    let mut grpc_req = tonic::Request::new($payload);
                    for (key, value) in &static_headers {
                        grpc_req.metadata_mut().insert(key.clone(), value.clone());
                    }
                    for (key, value) in &req.dynamic_headers {
                        grpc_req.metadata_mut().insert(key.clone(), value.clone());
                    }
                    $client
                        .export(grpc_req)
                        .map_err(|source| OtlpGrpcError::Request { source })
                        .await?;
                    len
                }};
            }

            let byte_size = match req.signal {
                OtlpSignal::Logs(r) => export!(logs_client, r),
                OtlpSignal::Metrics(r) => export!(metrics_client, r),
                OtlpSignal::Traces(r) => export!(traces_client, r),
            };

            emit!(EndpointBytesSent {
                byte_size,
                protocol: &protocol,
                endpoint: &endpoint,
            });

            Ok(OtlpGrpcResponse { events_byte_size })
        };

        Box::pin(future.map_err(|err: OtlpGrpcError| -> crate::Error { Box::new(err) }))
    }
}

// ── HyperSvc (same as in sinks/vector/service.rs) ────────────────────────────

#[derive(Clone)]
pub struct HyperSvc {
    uri: Uri,
    client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
}

impl Service<hyper::Request<BoxBody>> for HyperSvc {
    type Response = hyper::Response<hyper::Body>;
    type Error = hyper::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: hyper::Request<BoxBody>) -> Self::Future {
        // SAFETY: `self.uri` is always produced by `with_default_scheme`, which guarantees
        // a scheme and a path_and_query. Tonic always sets a path on the request URI.
        let uri = Uri::builder()
            .scheme(self.uri.scheme().expect("uri always has a scheme").clone())
            .authority(
                self.uri
                    .authority()
                    .expect("uri always has an authority")
                    .clone(),
            )
            .path_and_query(
                req.uri()
                    .path_and_query()
                    .expect("tonic request always has a path")
                    .clone(),
            )
            .build()
            .expect("uri components are always valid");

        *req.uri_mut() = uri;

        Box::pin(self.client.request(req))
    }
}

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
        self.signal.encoded_len()
    }
}

/// OTLP signal payload. Used as per-event intermediate data before batching and as the
/// request payload sent via gRPC after batching. Each variant corresponds to one signal
/// type so that requests can be retried independently.
#[derive(Clone)]
pub enum OtlpSignal {
    Logs(ExportLogsServiceRequest),
    Metrics(ExportMetricsServiceRequest),
    Traces(ExportTraceServiceRequest),
}

impl OtlpSignal {
    fn encoded_len(&self) -> usize {
        match self {
            OtlpSignal::Logs(r) => r.encoded_len(),
            OtlpSignal::Metrics(r) => r.encoded_len(),
            OtlpSignal::Traces(r) => r.encoded_len(),
        }
    }
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

        macro_rules! accumulate {
            ($field:ident, $req:expr, $merge:expr) => {
                match &mut self.$field {
                    Some(existing) => {
                        $merge(&mut existing.request, $req);
                        existing.finalizers.merge(finalizers);
                        existing.event_count += 1;
                        existing.byte_size += byte_size;
                        existing.json_byte_size += json_byte_size;
                    }
                    slot => {
                        *slot = Some(SignalData {
                            request: $req,
                            finalizers,
                            event_count: 1,
                            byte_size,
                            json_byte_size,
                        });
                    }
                }
            };
        }

        match signal {
            OtlpSignal::Logs(req) => accumulate!(
                logs,
                req,
                |e: &mut ExportLogsServiceRequest, r: ExportLogsServiceRequest| {
                    e.resource_logs.extend(r.resource_logs)
                }
            ),
            OtlpSignal::Metrics(req) => accumulate!(
                metrics,
                req,
                |e: &mut ExportMetricsServiceRequest, r: ExportMetricsServiceRequest| {
                    e.resource_metrics.extend(r.resource_metrics)
                }
            ),
            OtlpSignal::Traces(req) => accumulate!(
                traces,
                req,
                |e: &mut ExportTraceServiceRequest, r: ExportTraceServiceRequest| {
                    e.resource_spans.extend(r.resource_spans)
                }
            ),
        }
    }
}

pub struct OtlpGrpcSink<S> {
    pub batch_settings: BatcherSettings,
    pub service: S,
    uri_template: Template,
    use_https: bool,
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
        let use_https = self.use_https;
        let dynamic_header_templates = self.dynamic_header_templates.clone();

        input
            .filter_map(move |mut event| {
                macro_rules! drop_event {
                    ($reason:expr) => {{
                        let reason_owned: String = $reason.to_string();
                        emit!(crate::internal_events::SinkRequestBuildError {
                            error: reason_owned.as_str()
                        });
                        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                            count: 1,
                            reason: reason_owned.as_str(),
                        });
                        return futures::future::ready(None);
                    }};
                }

                let uri = match uri_template.render_string(&event) {
                    Ok(rendered) => match rendered.parse::<Uri>() {
                        Ok(parsed) => match with_default_scheme(parsed, use_https) {
                            Ok(u) => {
                                match u.scheme_str() {
                                    Some("https") if !use_https => {
                                        // The Hyper client was built without TLS (use_https=false),
                                        // so it cannot complete a TLS handshake. Sending data to an
                                        // https:// endpoint over a plaintext connector would either
                                        // fail or silently transmit unencrypted. Drop the event and
                                        // surface a clear error so the operator can add `tls:` or
                                        // use a static `https://` scheme prefix.
                                        drop_event!(
                                            "rendered gRPC URI uses \"https\" but the sink \
                                             has no TLS connector; add a `tls:` block or use \
                                             a static \"https://\" URI prefix so TLS is \
                                             enabled at startup"
                                        );
                                    }
                                    Some("http") | Some("https") => u,
                                    other => {
                                        drop_event!(format!(
                                            "rendered gRPC URI has disallowed scheme {:?}; only \"http\" and \"https\" are permitted",
                                            other.unwrap_or("<none>")
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                drop_event!(format!("invalid gRPC URI after rendering template: {e}"));
                            }
                        },
                        Err(e) => {
                            drop_event!(format!("failed to parse rendered gRPC URI: {e}"));
                        }
                    },
                    Err(e) => {
                        drop_event!(format!("failed to render gRPC URI template: {e}"));
                    }
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
                                    drop_event!(format!(
                                        "gRPC metadata value for key {:?} is not valid ASCII: {e}",
                                        key.as_str()
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            drop_event!(format!(
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
                        drop_event!(
                            "event is not OTLP-encoded (missing resourceLogs, resourceMetrics, \
                             or resourceSpans field); this sink only accepts OTLP-structured events"
                        );
                    }
                    Err(e) => {
                        drop_event!(e);
                    }
                }
            })
            // Partition by (rendered URI, dynamic headers) so that events destined for
            // different collectors or with different tenant metadata are never merged into
            // the same batch. Each partition is independently flushed by size or timeout.
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

                macro_rules! push_signal {
                    ($field:ident, $variant:ident) => {
                        if let Some(data) = batch.$field {
                            let byte_size = data.request.encoded_len();
                            let bytes_len =
                                NonZeroUsize::new(byte_size.max(1)).expect("should be non-zero");
                            let builder = RequestMetadataBuilder::new(
                                data.event_count,
                                data.byte_size,
                                data.json_byte_size,
                            );
                            requests.push(OtlpGrpcRequest {
                                signal: OtlpSignal::$variant(data.request),
                                uri: uri.clone(),
                                dynamic_headers: dynamic_headers.clone(),
                                finalizers: data.finalizers,
                                metadata: builder.with_request_size(bytes_len),
                            });
                        }
                    };
                }

                push_signal!(logs, Logs);
                push_signal!(metrics, Metrics);
                push_signal!(traces, Traces);

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
    let signal_type = detect_signal_type(event);
    let signal_type = match signal_type {
        Some(t) => t,
        None => return Ok(None),
    };

    let mut buf = BytesMut::new();
    // Clone the event to pass ownership to the encoder while keeping the original
    // for metadata extraction in the caller. The clone is shallow for logs/traces.
    serializer.encode(event.clone(), &mut buf)?;

    let bytes = buf.freeze();
    match signal_type {
        SignalType::Logs => {
            let req = ExportLogsServiceRequest::decode(bytes)?;
            Ok(Some(OtlpSignal::Logs(req)))
        }
        SignalType::Metrics => {
            let req = ExportMetricsServiceRequest::decode(bytes)?;
            Ok(Some(OtlpSignal::Metrics(req)))
        }
        SignalType::Traces => {
            let req = ExportTraceServiceRequest::decode(bytes)?;
            Ok(Some(OtlpSignal::Traces(req)))
        }
    }
}

enum SignalType {
    Logs,
    Metrics,
    Traces,
}

fn detect_signal_type(event: &Event) -> Option<SignalType> {
    match event {
        Event::Log(log) => {
            if log.contains(RESOURCE_LOGS_JSON_FIELD) {
                Some(SignalType::Logs)
            } else if log.contains(RESOURCE_METRICS_JSON_FIELD) {
                Some(SignalType::Metrics)
            } else {
                None
            }
        }
        Event::Trace(trace) => {
            if trace.contains(RESOURCE_SPANS_JSON_FIELD) {
                Some(SignalType::Traces)
            } else {
                None
            }
        }
        Event::Metric(_) => None, // Native Vector metrics not supported by OtlpSerializer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
