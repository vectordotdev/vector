use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use futures_util::FutureExt;
use http::StatusCode;
use hyper::{Body, Request as HttpRequest, Server, service::make_service_fn};
use prost::Message;
use snafu::Snafu;
use tokio::net::TcpStream;
use tower::{Service, ServiceBuilder};
use tracing::Span;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::decoding::{OtlpDeserializer, format::Deserializer},
    config::LogNamespace,
    event::{BatchNotifier, BatchStatus},
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Registered,
    },
    opentelemetry::proto::collector::{
        logs::v1::{ExportLogsServiceRequest, ExportLogsServiceResponse},
        metrics::v1::{ExportMetricsServiceRequest, ExportMetricsServiceResponse},
        trace::v1::{ExportTraceServiceRequest, ExportTraceServiceResponse},
    },
    tls::MaybeTlsIncomingStream,
};
use warp::{
    Filter, Reply, filters::BoxedFilter, http::HeaderMap, reject::Rejection, reply::Response,
};

use super::{reply::protobuf, status::Status};
use crate::{
    SourceSender,
    common::http::ErrorMessage,
    event::Event,
    http::{KeepaliveConfig, MaxConnectionAgeLayer, build_http_trace_layer},
    internal_events::{EventsReceived, HttpBadRequest, StreamClosedError},
    shutdown::ShutdownSignal,
    sources::{
        http_server::HttpConfigParamKind,
        opentelemetry::config::{LOGS, METRICS, OpentelemetryConfig, TRACES},
        util::{
            add_headers, decompress_body,
            http::capped_body,
            request_limiter::{RequestLimiter, RequestLimiterPermit},
        },
    },
    tls::{MaybeTlsSettings, TlsAcceptorReloader},
};

#[derive(Clone, Copy, Debug, Snafu)]
pub(crate) enum ApiError {
    ServerShutdown,
}

impl warp::reject::Reject for ApiError {}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_http_server(
    address: SocketAddr,
    tls_settings: MaybeTlsSettings,
    tls_reloader: Option<TlsAcceptorReloader>,
    filters: BoxedFilter<(Response,)>,
    shutdown: ShutdownSignal,
    keepalive_settings: KeepaliveConfig,
    request_limiter: RequestLimiter,
    request_timeout: Duration,
) -> crate::Result<()> {
    let listener = tls_settings
        .bind_reloadable(&address, tls_reloader)
        .await?
        .with_keepalive(keepalive_settings.tcp_keepalive);
    let routes = filters.recover(handle_rejection);

    info!(message = "Building HTTP server.", address = %address);

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
            .service(HttpRequestLimiterService {
                inner: warp::service(routes.clone()),
                request_limiter: request_limiter.clone(),
                request_timeout,
            });
        futures_util::future::ok::<_, Infallible>(svc)
    });

    Server::builder(hyper::server::accept::from_stream(listener.accept_stream()))
        .serve(make_svc)
        .with_graceful_shutdown(shutdown.map(|_| ()))
        .await?;

    Ok(())
}

#[derive(Clone)]
struct HttpRequestLimiterService<S> {
    inner: S,
    request_limiter: RequestLimiter,
    request_timeout: Duration,
}

impl<S> Service<HttpRequest<Body>> for HttpRequestLimiterService<S>
where
    S: Service<HttpRequest<Body>, Response = Response> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = futures_util::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: HttpRequest<Body>) -> Self::Future {
        if !is_otlp_http_request(&request) {
            return self.inner.call(request).boxed();
        }

        match self.request_limiter.try_acquire() {
            Some(permit) => {
                let permit = Arc::new(permit);
                request.extensions_mut().insert(Arc::clone(&permit));
                let future = self.inner.call(request);
                let request_timeout = self.request_timeout;
                async move {
                    let response = match tokio::time::timeout(request_timeout, future).await {
                        Ok(response) => return response,
                        Err(_) => otlp_error_response(
                            StatusCode::SERVICE_UNAVAILABLE,
                            tonic::Code::Unavailable,
                            "request processing timed out",
                        ),
                    };
                    drop(permit);
                    Ok(response)
                }
                .boxed()
            }
            None => futures_util::future::ready(Ok(otlp_error_response(
                StatusCode::TOO_MANY_REQUESTS,
                tonic::Code::Unavailable,
                "too many concurrent requests",
            )))
            .boxed(),
        }
    }
}

fn is_otlp_http_request(request: &HttpRequest<Body>) -> bool {
    request.method() == http::Method::POST
        && matches!(
            request.uri().path(),
            "/v1/logs"
                | "/v1/logs/"
                | "/v1/metrics"
                | "/v1/metrics/"
                | "/v1/traces"
                | "/v1/traces/"
        )
        && request
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.eq_ignore_ascii_case("application/x-protobuf"))
}

fn otlp_error_response(status_code: StatusCode, code: tonic::Code, message: &str) -> Response {
    let mut response = protobuf(Status {
        code: code as i32,
        message: message.into(),
        ..Default::default()
    })
    .into_response();
    *response.status_mut() = status_code;
    response
}

#[allow(clippy::too_many_arguments)] // TODO change to a builder struct
pub(crate) fn build_warp_filter(
    acknowledgements: bool,
    log_namespace: LogNamespace,
    out: SourceSender,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
    headers: Vec<HttpConfigParamKind>,
    logs_deserializer: Option<OtlpDeserializer>,
    metrics_deserializer: Option<OtlpDeserializer>,
    traces_deserializer: Option<OtlpDeserializer>,
) -> BoxedFilter<(Response,)> {
    let log_filters = build_warp_log_filter(
        acknowledgements,
        log_namespace,
        out.clone(),
        bytes_received.clone(),
        events_received.clone(),
        headers.clone(),
        logs_deserializer,
    );
    let metrics_filters = build_warp_metrics_filter(
        acknowledgements,
        log_namespace,
        out.clone(),
        bytes_received.clone(),
        events_received.clone(),
        headers.clone(),
        metrics_deserializer,
    );
    let trace_filters = build_warp_trace_filter(
        acknowledgements,
        out,
        bytes_received,
        events_received,
        headers,
        traces_deserializer,
    );
    log_filters
        .or(trace_filters)
        .unify()
        .or(metrics_filters)
        .unify()
        .boxed()
}

fn enrich_events(
    events: &mut [Event],
    headers_config: &[HttpConfigParamKind],
    headers: &HeaderMap,
    log_namespace: LogNamespace,
) {
    add_headers(
        events,
        headers_config,
        headers,
        log_namespace,
        OpentelemetryConfig::NAME,
    );
}

fn emit_decode_error(error: impl std::fmt::Display) -> ErrorMessage {
    let message = format!("Could not decode request: {error}");
    emit!(HttpBadRequest::new(
        StatusCode::BAD_REQUEST.as_u16(),
        &message
    ));
    ErrorMessage::new(StatusCode::BAD_REQUEST, message)
}

struct DecodedEvents {
    events: Vec<Event>,
    count: usize,
}

fn parse_with_deserializer(
    deserializer: &OtlpDeserializer,
    body: Bytes,
    log_namespace: LogNamespace,
    events_received: &Registered<EventsReceived>,
) -> Result<DecodedEvents, ErrorMessage> {
    let events = deserializer
        .parse(body, log_namespace)
        .map(|r| r.into_vec())
        .map_err(emit_decode_error)?;

    // Count individual items within OTLP batches for consistency with other sources
    let count = super::count_otlp_items(&events);
    events_received.emit(CountByteSize(
        count,
        events.estimated_json_encoded_size_of(),
    ));

    Ok(DecodedEvents { events, count })
}

fn build_ingest_filter<Resp, F>(
    telemetry_type: &'static str,
    acknowledgements: bool,
    out: SourceSender,
    make_events: F,
) -> BoxedFilter<(Response,)>
where
    Resp: prost::Message + Default + Send + 'static,
    F: Clone
        + Send
        + Sync
        + 'static
        + Fn(Option<String>, HeaderMap, Bytes) -> Result<DecodedEvents, ErrorMessage>,
{
    let body_filter = capped_body();
    warp::post()
        .and(warp::path("v1"))
        .and(warp::path(telemetry_type))
        .and(warp::path::end())
        .and(warp::header::exact_ignore_case(
            "content-type",
            "application/x-protobuf",
        ))
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::header::headers_cloned())
        .and(warp::filters::ext::get::<Arc<RequestLimiterPermit>>())
        .and(body_filter)
        .and_then(
            move |encoding_header: Option<String>,
                  headers: HeaderMap,
                  permit: Arc<RequestLimiterPermit>,
                  body: Bytes| {
                let events = make_events(encoding_header, headers, body);
                let out = out.clone();
                async move {
                    handle_request(
                        events,
                        acknowledgements,
                        out,
                        telemetry_type,
                        Resp::default(),
                        permit,
                    )
                    .await
                }
            },
        )
        .boxed()
}

#[allow(clippy::too_many_arguments)]
fn build_warp_log_filter(
    acknowledgements: bool,
    log_namespace: LogNamespace,
    source_sender: SourceSender,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
    headers_cfg: Vec<HttpConfigParamKind>,
    deserializer: Option<OtlpDeserializer>,
) -> BoxedFilter<(Response,)> {
    let make_events = move |encoding_header: Option<String>, headers: HeaderMap, body: Bytes| {
        decompress_body(encoding_header.as_deref(), body)
            .inspect_err(|err| {
                // Other status codes are already handled by `sources::util::decompress_body` (tech debt).
                if err.status_code() == StatusCode::UNSUPPORTED_MEDIA_TYPE {
                    emit!(HttpBadRequest::new(
                        err.status_code().as_u16(),
                        err.message()
                    ));
                }
            })
            .and_then(|decoded_body| {
                bytes_received.emit(ByteSize(decoded_body.len()));
                if let Some(d) = deserializer.as_ref() {
                    parse_with_deserializer(d, decoded_body, log_namespace, &events_received)
                } else {
                    decode_log_body(decoded_body, log_namespace, &events_received)
                }
                .map(|mut decoded| {
                    enrich_events(&mut decoded.events, &headers_cfg, &headers, log_namespace);
                    decoded
                })
            })
    };

    build_ingest_filter::<ExportLogsServiceResponse, _>(
        LOGS,
        acknowledgements,
        source_sender,
        make_events,
    )
}
#[allow(clippy::too_many_arguments)]
fn build_warp_metrics_filter(
    acknowledgements: bool,
    log_namespace: LogNamespace,
    source_sender: SourceSender,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
    headers_cfg: Vec<HttpConfigParamKind>,
    deserializer: Option<OtlpDeserializer>,
) -> BoxedFilter<(Response,)> {
    let make_events = move |encoding_header: Option<String>, headers: HeaderMap, body: Bytes| {
        decompress_body(encoding_header.as_deref(), body)
            .inspect_err(|err| {
                // Other status codes are already handled by `sources::util::decompress_body` (tech debt).
                if err.status_code() == StatusCode::UNSUPPORTED_MEDIA_TYPE {
                    emit!(HttpBadRequest::new(
                        err.status_code().as_u16(),
                        err.message()
                    ));
                }
            })
            .and_then(|decoded_body| {
                bytes_received.emit(ByteSize(decoded_body.len()));
                if let Some(d) = deserializer.as_ref() {
                    parse_with_deserializer(d, decoded_body, log_namespace, &events_received)
                } else {
                    decode_metrics_body(decoded_body, &events_received)
                }
                .map(|mut decoded| {
                    enrich_events(&mut decoded.events, &headers_cfg, &headers, log_namespace);
                    decoded
                })
            })
    };

    build_ingest_filter::<ExportMetricsServiceResponse, _>(
        METRICS,
        acknowledgements,
        source_sender,
        make_events,
    )
}

fn build_warp_trace_filter(
    acknowledgements: bool,
    source_sender: SourceSender,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
    headers_cfg: Vec<HttpConfigParamKind>,
    deserializer: Option<OtlpDeserializer>,
) -> BoxedFilter<(Response,)> {
    let make_events = move |encoding_header: Option<String>, headers: HeaderMap, body: Bytes| {
        decompress_body(encoding_header.as_deref(), body)
            .inspect_err(|err| {
                // Other status codes are already handled by `sources::util::decompress_body` (tech debt).
                if err.status_code() == StatusCode::UNSUPPORTED_MEDIA_TYPE {
                    emit!(HttpBadRequest::new(
                        err.status_code().as_u16(),
                        err.message()
                    ));
                }
            })
            .and_then(|decoded_body| {
                bytes_received.emit(ByteSize(decoded_body.len()));
                if let Some(d) = deserializer.as_ref() {
                    parse_with_deserializer(
                        d,
                        decoded_body,
                        LogNamespace::default(),
                        &events_received,
                    )
                } else {
                    decode_trace_body(decoded_body, &events_received)
                }
                .map(|mut decoded| {
                    enrich_events(
                        &mut decoded.events,
                        &headers_cfg,
                        &headers,
                        LogNamespace::default(),
                    );
                    decoded
                })
            })
    };

    build_ingest_filter::<ExportTraceServiceResponse, _>(
        TRACES,
        acknowledgements,
        source_sender,
        make_events,
    )
}

fn decode_trace_body(
    body: Bytes,
    events_received: &Registered<EventsReceived>,
) -> Result<DecodedEvents, ErrorMessage> {
    let request = ExportTraceServiceRequest::decode(body).map_err(emit_decode_error)?;

    let events: Vec<Event> = request
        .resource_spans
        .into_iter()
        .flat_map(|v| v.into_event_iter())
        .collect();

    let count = events.len();
    events_received.emit(CountByteSize(
        count,
        events.estimated_json_encoded_size_of(),
    ));

    Ok(DecodedEvents { events, count })
}

fn decode_log_body(
    body: Bytes,
    log_namespace: LogNamespace,
    events_received: &Registered<EventsReceived>,
) -> Result<DecodedEvents, ErrorMessage> {
    let request = ExportLogsServiceRequest::decode(body).map_err(emit_decode_error)?;

    let events: Vec<Event> = request
        .resource_logs
        .into_iter()
        .flat_map(|v| v.into_event_iter(log_namespace))
        .collect();

    let count = events.len();
    events_received.emit(CountByteSize(
        count,
        events.estimated_json_encoded_size_of(),
    ));

    Ok(DecodedEvents { events, count })
}

fn decode_metrics_body(
    body: Bytes,
    events_received: &Registered<EventsReceived>,
) -> Result<DecodedEvents, ErrorMessage> {
    let request = ExportMetricsServiceRequest::decode(body).map_err(emit_decode_error)?;

    let events: Vec<Event> = request
        .resource_metrics
        .into_iter()
        .flat_map(|v| v.into_event_iter())
        .collect();

    let count = events.len();
    events_received.emit(CountByteSize(
        count,
        events.estimated_json_encoded_size_of(),
    ));

    Ok(DecodedEvents { events, count })
}

async fn handle_request(
    events: Result<DecodedEvents, ErrorMessage>,
    acknowledgements: bool,
    mut out: SourceSender,
    output: &str,
    resp: impl Message,
    permit: Arc<RequestLimiterPermit>,
) -> Result<Response, Rejection> {
    match events {
        Ok(mut decoded) => {
            permit.decoding_finished(decoded.count);
            let receiver = BatchNotifier::maybe_apply_to(acknowledgements, &mut decoded.events);
            let count = decoded.events.len();

            out.send_batch_named(output, decoded.events)
                .await
                .map_err(|_| {
                    emit!(StreamClosedError { count });
                    warp::reject::custom(ApiError::ServerShutdown)
                })?;

            match receiver {
                None => Ok(protobuf(resp).into_response()),
                Some(receiver) => match receiver.await {
                    BatchStatus::Delivered => Ok(protobuf(resp).into_response()),
                    BatchStatus::Errored => Err(warp::reject::custom(Status {
                        code: 2, // UNKNOWN - OTLP doesn't require use of status.code, but we can't encode a None here
                        message: "Error delivering contents to sink".into(),
                        ..Default::default()
                    })),
                    BatchStatus::Rejected => Err(warp::reject::custom(Status {
                        code: 2, // UNKNOWN - OTLP doesn't require use of status.code, but we can't encode a None here
                        message: "Contents failed to deliver to sink".into(),
                        ..Default::default()
                    })),
                },
            }
        }
        Err(err) => Err(warp::reject::custom(err)),
    }
}

async fn handle_rejection(err: Rejection) -> Result<impl Reply, std::convert::Infallible> {
    if let Some(err_msg) = err.find::<ErrorMessage>() {
        let reply = protobuf(Status {
            code: 2, // UNKNOWN - OTLP doesn't require use of status.code, but we can't encode a None here
            message: err_msg.message().into(),
            ..Default::default()
        });

        Ok(warp::reply::with_status(reply, err_msg.status_code()))
    } else {
        let reply = protobuf(Status {
            code: 2, // UNKNOWN - OTLP doesn't require use of status.code, but we can't encode a None here
            message: format!("{err:?}"),
            ..Default::default()
        });

        Ok(warp::reply::with_status(
            reply,
            StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use hyper::body::HttpBody as _;

    use super::*;

    fn request(uri: &str, body: Body) -> HttpRequest<Body> {
        HttpRequest::post(uri)
            .header(http::header::CONTENT_TYPE, "application/x-protobuf")
            .body(body)
            .unwrap()
    }

    #[test]
    fn identifies_warp_otlp_paths() {
        for path in [
            "/v1/logs",
            "/v1/logs/",
            "/v1/logs?client=test",
            "/v1/logs/?client=test",
            "/v1/metrics",
            "/v1/metrics/",
            "/v1/traces",
            "/v1/traces/",
        ] {
            assert!(
                is_otlp_http_request(&request(path, Body::empty())),
                "{path}"
            );
        }

        for path in [
            "/v1/logs//",
            "/v1/logs/extra",
            "/v1/metrics//",
            "/v1/traces/extra",
        ] {
            assert!(
                !is_otlp_http_request(&request(path, Body::empty())),
                "{path}"
            );
        }
    }

    #[tokio::test]
    async fn stalled_body_times_out_and_releases_permit() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner_calls = Arc::clone(&calls);
        let inner = tower::service_fn(move |request: HttpRequest<Body>| {
            let calls = Arc::clone(&inner_calls);
            async move {
                calls.fetch_add(1, Ordering::Relaxed);
                request.into_body().collect().await.unwrap().to_bytes();
                Ok::<_, Infallible>(Response::new(Body::empty()))
            }
        });
        let mut service = HttpRequestLimiterService {
            inner,
            request_limiter: RequestLimiter::new(100, 1),
            request_timeout: Duration::from_millis(10),
        };
        let (body_sender, stalled_body) = Body::channel();

        let response = service
            .call(request("/v1/logs", stalled_body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let status =
            Status::decode(response.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(status.code, tonic::Code::Unavailable as i32);
        assert_eq!(status.message, "request processing timed out");

        let response = service
            .call(request("/v1/logs", Body::empty()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        drop(body_sender);
    }
}
