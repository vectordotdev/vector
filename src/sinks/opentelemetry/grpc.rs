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
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::{BatcherSettings, DriverResponse, batcher::data::BatchReduce},
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
        parts.scheme = if tls {
            Some(
                "https"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("https should be valid")),
            )
        } else {
            Some(
                "http"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("http should be valid")),
            )
        };
        if parts.path_and_query.is_none() {
            parts.path_and_query = Some(
                "/".parse()
                    .unwrap_or_else(|_| unreachable!("root should be valid")),
            );
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
        if self.uri.is_dynamic() {
            return Err(
                "template syntax is not supported for `uri` with the gRPC transport; \
                 use a literal URI (e.g. `http://localhost:4317`)"
                    .into(),
            );
        }

        let uri = with_default_scheme(
            self.uri
                .get_ref()
                .parse()
                .map_err(|e| format!("invalid URI for gRPC sink: {e}"))?,
            self.tls.is_some(),
        )?;

        let tls = if uri.scheme_str() == Some("https") {
            MaybeTlsSettings::tls_client(self.tls.as_ref())?
        } else {
            MaybeTlsSettings::Raw(())
        };

        let use_gzip = self.compression == GrpcCompression::Gzip;

        let grpc_headers: Vec<(
            tonic::metadata::AsciiMetadataKey,
            tonic::metadata::AsciiMetadataValue,
        )> = self
            .request
            .headers
            .iter()
            .filter_map(|(k, v)| {
                let key = tonic::metadata::AsciiMetadataKey::from_bytes(k.as_bytes())
                    .map_err(|e| warn!("Skipping invalid gRPC metadata key {k:?}: {e}"))
                    .ok()?;
                let value = tonic::metadata::AsciiMetadataValue::try_from(v.as_str())
                    .map_err(|e| warn!("Skipping invalid gRPC metadata value for {k:?}: {e}"))
                    .ok()?;
                Some((key, value))
            })
            .collect();

        let client = new_grpc_client(&tls, cx.proxy())?;
        let service = OtlpGrpcService::new(client.clone(), uri.clone(), use_gzip, grpc_headers);

        let healthcheck = Box::pin(grpc_healthcheck(client, uri, cx.healthcheck));

        let request_settings = self.request.tower.into_settings();
        let batch_settings = self.batch.into_batcher_settings()?;

        let service = ServiceBuilder::new()
            .settings(request_settings, OtlpGrpcRetryLogic)
            .service(service);

        let sink = OtlpGrpcSink {
            batch_settings,
            service,
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
    uri: Uri,
    options: SinkHealthcheckOptions,
) -> crate::Result<()> {
    if !options.enabled {
        return Ok(());
    }

    use tonic::Code;
    use tonic_health::pb::{HealthCheckRequest, health_client::HealthClient};

    let svc = HyperSvc { uri, client };
    let mut health_client = HealthClient::new(svc);

    match health_client
        .check(HealthCheckRequest {
            service: String::new(),
        })
        .await
    {
        Ok(response) => {
            use tonic_health::pb::health_check_response::ServingStatus;
            let status = response.into_inner().status;
            if status == ServingStatus::Serving as i32 {
                Ok(())
            } else {
                Err(format!("gRPC collector reported non-serving status: {status}").into())
            }
        }
        // Server is reachable but does not implement the health protocol — treat as healthy.
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
/// that retries are atomic — a failed metrics export cannot duplicate a
/// previously-accepted logs export.
#[derive(Clone)]
pub struct OtlpGrpcRequest {
    pub signal: OtlpSignalRequest,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
}

/// The OTLP export payload for a single signal type.
#[derive(Clone)]
pub enum OtlpSignalRequest {
    Logs(ExportLogsServiceRequest),
    Metrics(ExportMetricsServiceRequest),
    Traces(ExportTraceServiceRequest),
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

#[derive(Clone)]
pub struct OtlpGrpcService {
    logs_client: LogsServiceClient<HyperSvc>,
    metrics_client: MetricsServiceClient<HyperSvc>,
    traces_client: TraceServiceClient<HyperSvc>,
    headers: Vec<(tonic::metadata::AsciiMetadataKey, tonic::metadata::AsciiMetadataValue)>,
    protocol: String,
    endpoint: String,
}

impl OtlpGrpcService {
    pub fn new(
        hyper_client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
        uri: Uri,
        compression: bool,
        headers: Vec<(tonic::metadata::AsciiMetadataKey, tonic::metadata::AsciiMetadataValue)>,
    ) -> Self {
        let (protocol, endpoint) = crate::sinks::util::uri::protocol_endpoint(uri.clone());

        let svc = HyperSvc {
            uri,
            client: hyper_client,
        };

        let mut logs_client = LogsServiceClient::new(svc.clone());
        let mut metrics_client = MetricsServiceClient::new(svc.clone());
        let mut traces_client = TraceServiceClient::new(svc);

        if compression {
            logs_client = logs_client.send_compressed(tonic::codec::CompressionEncoding::Gzip);
            metrics_client =
                metrics_client.send_compressed(tonic::codec::CompressionEncoding::Gzip);
            traces_client = traces_client.send_compressed(tonic::codec::CompressionEncoding::Gzip);
        }

        Self {
            logs_client,
            metrics_client,
            traces_client,
            headers,
            protocol,
            endpoint,
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
        let mut svc = self.clone();
        let metadata = std::mem::take(req.metadata_mut());
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();

        let future = async move {
            macro_rules! export {
                ($client:expr, $payload:expr) => {{
                    let len = $payload.encoded_len();
                    let mut grpc_req = tonic::Request::new($payload);
                    for (key, value) in &svc.headers {
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
                OtlpSignalRequest::Logs(r) => export!(svc.logs_client, r),
                OtlpSignalRequest::Metrics(r) => export!(svc.metrics_client, r),
                OtlpSignalRequest::Traces(r) => export!(svc.traces_client, r),
            };

            emit!(EndpointBytesSent {
                byte_size,
                protocol: &svc.protocol,
                endpoint: &svc.endpoint,
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
        let uri = Uri::builder()
            .scheme(self.uri.scheme().unwrap().clone())
            .authority(self.uri.authority().unwrap().clone())
            .path_and_query(req.uri().path_and_query().unwrap().clone())
            .build()
            .unwrap();

        *req.uri_mut() = uri;

        Box::pin(self.client.request(req))
    }
}

// ── Sink ─────────────────────────────────────────────────────────────────────

/// Intermediate event data extracted before batching.
struct OtlpEventData {
    byte_size: usize,
    json_byte_size: GroupedCountByteSize,
    finalizers: EventFinalizers,
    signal: OtlpSignal,
}

/// Pre-decoded OTLP signal for a single event.
enum OtlpSignal {
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

impl Clone for OtlpBatch {
    fn clone(&self) -> Self {
        // OtlpBatch is used only in the batcher accumulator; Clone is required by the
        // BatchReduce API but the accumulator is always the initial default value.
        Self::default()
    }
}

pub struct OtlpGrpcSink<S> {
    pub batch_settings: BatcherSettings,
    pub service: S,
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

        input
            .filter_map(|mut event| {
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
                        };
                        futures::future::ready(Some(data))
                    }
                    Ok(None) => {
                        // Event type not supported (e.g. native Vector Metric)
                        emit!(crate::internal_events::SinkRequestBuildError {
                            error: "Unsupported event type for OTLP gRPC sink (native Vector Metric events are not supported; use OTLP-decoded metrics)",
                        });
                        futures::future::ready(None)
                    }
                    Err(e) => {
                        emit!(crate::internal_events::SinkRequestBuildError { error: e });
                        futures::future::ready(None)
                    }
                }
            })
            .batched(self.batch_settings.as_reducer_config(
                |data: &OtlpEventData| data.signal.encoded_len(),
                BatchReduce::new(|batch: &mut OtlpBatch, item: OtlpEventData| {
                    macro_rules! accumulate {
                        ($field:ident, $req:expr, $merge:expr) => {
                            match &mut batch.$field {
                                Some(existing) => {
                                    $merge(&mut existing.request, $req);
                                    existing.finalizers.merge(item.finalizers);
                                    existing.event_count += 1;
                                    existing.byte_size += item.byte_size;
                                    existing.json_byte_size += item.json_byte_size;
                                }
                                slot => {
                                    *slot = Some(SignalData {
                                        request: $req,
                                        finalizers: item.finalizers,
                                        event_count: 1,
                                        byte_size: item.byte_size,
                                        json_byte_size: item.json_byte_size,
                                    });
                                }
                            }
                        };
                    }
                    match item.signal {
                        OtlpSignal::Logs(req) => accumulate!(logs, req, |e: &mut ExportLogsServiceRequest, r: ExportLogsServiceRequest| {
                            e.resource_logs.extend(r.resource_logs)
                        }),
                        OtlpSignal::Metrics(req) => accumulate!(metrics, req, |e: &mut ExportMetricsServiceRequest, r: ExportMetricsServiceRequest| {
                            e.resource_metrics.extend(r.resource_metrics)
                        }),
                        OtlpSignal::Traces(req) => accumulate!(traces, req, |e: &mut ExportTraceServiceRequest, r: ExportTraceServiceRequest| {
                            e.resource_spans.extend(r.resource_spans)
                        }),
                    }
                }),
            ))
            .flat_map(|batch| {
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
                                signal: OtlpSignalRequest::$variant(data.request),
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
