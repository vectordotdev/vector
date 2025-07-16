use std::time::Duration;
use std::{convert::Infallible, net::SocketAddr};

use bytes::Buf;
use bytes::Bytes;
use futures_util::FutureExt;
use http::StatusCode;
use hyper::{service::make_service_fn, Server};
use tokio::net::TcpStream;
use tower::ServiceBuilder;
use tracing::Span;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Registered,
};
use vector_lib::opentelemetry::proto::collector::{
    logs::v1::{ExportLogsServiceRequest, ExportLogsServiceResponse},
    metrics::v1::{ExportMetricsServiceRequest, ExportMetricsServiceResponse},
    trace::v1::{ExportTraceServiceRequest, ExportTraceServiceResponse},
};
use vector_lib::tls::MaybeTlsIncomingStream;
use vector_lib::{
    config::LogNamespace,
    event::{BatchNotifier, BatchStatus},
    EstimatedJsonEncodedSizeOf,
};
use warp::{
    filters::BoxedFilter, http::HeaderMap, reject::Rejection, reply::Response, Filter, Reply,
};

use crate::common::http::ErrorMessage;
use crate::http::{KeepaliveConfig, MaxConnectionAgeLayer};
use crate::sources::http_server::HttpConfigParamKind;
use crate::sources::util::add_headers;
use crate::{
    event::Event,
    http::build_http_trace_layer,
    internal_events::{EventsReceived, StreamClosedError},
    shutdown::ShutdownSignal,
    sources::util::decode,
    tls::MaybeTlsSettings,
    SourceSender,
};

use super::OpentelemetryConfig;
use super::{reply::protobuf, status::Status};

#[derive(Clone, Copy, Debug)]
pub(crate) enum ContentType {
    Protobuf,
    Json,
}

#[derive(Debug)]
struct InvalidContentType;

impl warp::reject::Reject for InvalidContentType {}

fn extract_content_type() -> impl warp::Filter<Extract = (ContentType,), Error = Rejection> + Copy {
    warp::header::<String>(http::header::CONTENT_TYPE.as_str()).and_then(
        |content_type: String| async move {
            match content_type.as_str() {
                "application/x-protobuf" => Ok(ContentType::Protobuf),
                "application/json" => Ok(ContentType::Json),
                _ => Err(warp::reject::custom(InvalidContentType)),
            }
        },
    )
}

pub(crate) async fn run_http_server(
    address: SocketAddr,
    tls_settings: MaybeTlsSettings,
    routes: BoxedFilter<(Response,)>,
    shutdown: ShutdownSignal,
    keepalive_settings: KeepaliveConfig,
) -> crate::Result<()> {
    let listener = tls_settings.bind(&address).await?;

    info!(message = "Building HTTP server.", address = %address);

    let routes = routes.recover(handle_rejection);

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
        .await?;

    Ok(())
}

pub(crate) fn build_warp_filter(
    acknowledgements: bool,
    log_namespace: LogNamespace,
    out: SourceSender,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
    headers: Vec<HttpConfigParamKind>,
) -> BoxedFilter<(Response,)> {
    let log_filters = build_warp_log_filter(
        acknowledgements,
        log_namespace,
        out.clone(),
        bytes_received.clone(),
        events_received.clone(),
        headers.clone(),
    );
    let metrics_filters = build_warp_metrics_filter(
        acknowledgements,
        out.clone(),
        bytes_received.clone(),
        events_received.clone(),
    );
    let trace_filters = build_warp_trace_filter(
        acknowledgements,
        out.clone(),
        bytes_received,
        events_received,
    );

    // The OTLP spec says that HTTP 4xx errors must include a grpc Status message
    // by doing it here we get access to the content-type header, which is required
    // to differentiate between protobuf and json encoding
    let handle_errors: BoxedFilter<(Response,)> = warp::any()
        .and(warp::method())
        .and(warp::path::peek())
        .and(extract_content_type())
        .then(
            |method, path: warp::filters::path::Peek, ct: ContentType| async move {
                if method != hyper::Method::POST {
                    let status = Status {
                        code: 2,
                        message: "method not allowed, supported: [POST]".into(),
                        ..Default::default()
                    };

                    serialize_response(ct, hyper::StatusCode::METHOD_NOT_ALLOWED, status)
                } else {
                    let status = Status {
                        code: 2,
                        message: format!("unknown route: {}", path.as_str()),
                        ..Default::default()
                    };

                    serialize_response(ct, hyper::StatusCode::NOT_FOUND, status)
                }
            },
        )
        .boxed();

    log_filters
        .or(trace_filters)
        .unify()
        .or(metrics_filters)
        .unify()
        .or(handle_errors)
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

fn build_warp_log_filter(
    acknowledgements: bool,
    log_namespace: LogNamespace,
    out: SourceSender,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
    headers: Vec<HttpConfigParamKind>,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(warp::path!("v1" / "logs"))
        .and(extract_content_type())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::header::headers_cloned())
        .and(warp::body::bytes())
        .and_then(
            move |ct: ContentType,
                  encoding_header: Option<String>,
                  headers_config: HeaderMap,
                  body: Bytes| {
                let events = decode(encoding_header.as_deref(), body)
                    .and_then(|body| {
                        bytes_received.emit(ByteSize(body.len()));
                        decode_log_body(body, log_namespace, &events_received, ct)
                    })
                    .map(|mut events| {
                        enrich_events(&mut events, &headers, &headers_config, log_namespace);
                        events
                    });

                handle_request::<ExportLogsServiceResponse>(
                    events,
                    acknowledgements,
                    out.clone(),
                    super::LOGS,
                    ct,
                )
            },
        )
        .boxed()
}

fn build_warp_metrics_filter(
    acknowledgements: bool,
    out: SourceSender,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(warp::path!("v1" / "metrics"))
        .and(extract_content_type())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::body::bytes())
        .and_then(
            move |ct: ContentType, encoding_header: Option<String>, body: Bytes| {
                let events = decode(encoding_header.as_deref(), body).and_then(|body| {
                    bytes_received.emit(ByteSize(body.len()));
                    decode_metrics_body(body, &events_received, ct)
                });

                handle_request::<ExportMetricsServiceResponse>(
                    events,
                    acknowledgements,
                    out.clone(),
                    super::METRICS,
                    ct,
                )
            },
        )
        .boxed()
}

fn build_warp_trace_filter(
    acknowledgements: bool,
    out: SourceSender,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(warp::path!("v1" / "traces"))
        .and(extract_content_type())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::body::bytes())
        .and_then(
            move |ct: ContentType, encoding_header: Option<String>, body: Bytes| {
                let events = decode(encoding_header.as_deref(), body).and_then(|body| {
                    bytes_received.emit(ByteSize(body.len()));
                    decode_trace_body(body, &events_received, ct)
                });

                handle_request::<ExportTraceServiceResponse>(
                    events,
                    acknowledgements,
                    out.clone(),
                    super::TRACES,
                    ct,
                )
            },
        )
        .boxed()
}

fn deserialize_payload<'a, T>(content_type: ContentType, body: Bytes) -> Result<T, ErrorMessage>
where
    T: prost::Message + std::default::Default + serde::de::DeserializeOwned,
{
    let data: T = match content_type {
        ContentType::Protobuf => T::decode(body).map_err(|error| {
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Could not decode request: {}", error),
            )
        }),
        ContentType::Json => serde_json::from_reader(body.reader()).map_err(|error| {
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Could not decode request: {}", error),
            )
        }),
    }?;
    Ok(data)
}

fn serialize_response<T: serde::Serialize + prost::Message>(
    content_type: ContentType,
    status_code: hyper::StatusCode,
    payload: T,
) -> Response {
    match content_type {
        ContentType::Json => {
            let mut resp = warp::reply::json(&payload).into_response();
            *resp.status_mut() = status_code;
            resp
        }
        ContentType::Protobuf => protobuf(status_code, payload).into_response(),
    }
}

fn decode_trace_body(
    body: Bytes,
    events_received: &Registered<EventsReceived>,
    content_type: ContentType,
) -> Result<Vec<Event>, ErrorMessage> {
    let request = deserialize_payload::<ExportTraceServiceRequest>(content_type, body)?;

    let events: Vec<Event> = request
        .resource_spans
        .into_iter()
        .flat_map(|v| v.into_event_iter())
        .collect();

    events_received.emit(CountByteSize(
        events.len(),
        events.estimated_json_encoded_size_of(),
    ));

    Ok(events)
}

fn decode_log_body(
    body: Bytes,
    log_namespace: LogNamespace,
    events_received: &Registered<EventsReceived>,
    content_type: ContentType,
) -> Result<Vec<Event>, ErrorMessage> {
    let request = deserialize_payload::<ExportLogsServiceRequest>(content_type, body)?;

    let events: Vec<Event> = request
        .resource_logs
        .into_iter()
        .flat_map(|v| v.into_event_iter(log_namespace))
        .collect();

    events_received.emit(CountByteSize(
        events.len(),
        events.estimated_json_encoded_size_of(),
    ));

    Ok(events)
}

fn decode_metrics_body(
    body: Bytes,
    events_received: &Registered<EventsReceived>,
    content_type: ContentType,
) -> Result<Vec<Event>, ErrorMessage> {
    let request = deserialize_payload::<ExportMetricsServiceRequest>(content_type, body)?;

    let events: Vec<Event> = request
        .resource_metrics
        .into_iter()
        .flat_map(|v| v.into_event_iter())
        .collect();

    events_received.emit(CountByteSize(
        events.len(),
        events.estimated_json_encoded_size_of(),
    ));

    Ok(events)
}

async fn handle_request<T>(
    events: Result<Vec<Event>, ErrorMessage>,
    acknowledgements: bool,
    mut out: SourceSender,
    output: &str,
    content_type: ContentType,
) -> Result<Response, Rejection>
where
    T: prost::Message + serde::Serialize + std::default::Default,
{
    let reply_with_status =
        |http_code: hyper::StatusCode, code: i32, message: String| -> Result<Response, Rejection> {
            let s = Status {
                code,
                message,
                ..Default::default()
            };
            Ok(serialize_response(content_type, http_code, s))
        };

    let mut events = match events {
        Err(err) => {
            return reply_with_status(
                err.status_code(),
                2, // UNKNOWN - OTLP doesn't require use of status.code, but we can't encode a None here
                err.message().into(),
            );
        }
        Ok(events) => events,
    };

    let receiver = BatchNotifier::maybe_apply_to(acknowledgements, &mut events);
    let count = events.len();

    if let Err(_) = out.send_batch_named(output, events).await {
        emit!(StreamClosedError { count });
        // the client can try again later
        return reply_with_status(
            hyper::StatusCode::SERVICE_UNAVAILABLE,
            2,
            "Server is shutting down".into(),
        );
    };

    match receiver {
        None => Ok(serialize_response(
            content_type,
            hyper::StatusCode::OK,
            T::default(),
        )),
        Some(receiver) => match receiver.await {
            BatchStatus::Delivered => Ok(serialize_response(
                content_type,
                hyper::StatusCode::OK,
                T::default(),
            )),
            BatchStatus::Errored => reply_with_status(
                hyper::StatusCode::INTERNAL_SERVER_ERROR,
                2, // UNKNOWN - OTLP doesn't require use of status.code, but we can't encode a None here
                "Error delivering contents to sink".into(),
            ),
            BatchStatus::Rejected => reply_with_status(
                hyper::StatusCode::INTERNAL_SERVER_ERROR,
                2, // UNKNOWN - OTLP doesn't require use of status.code, but we can't encode a None here
                "Contents failed to deliver to sink".into(),
            ),
        },
    }
}

async fn handle_rejection(err: Rejection) -> Result<Response, Infallible> {
    let reply = if let Some(_) = err.find::<InvalidContentType>() {
        warp::reply::with_status(
            hyper::StatusCode::UNSUPPORTED_MEDIA_TYPE.as_str(),
            hyper::StatusCode::UNSUPPORTED_MEDIA_TYPE,
        )
    } else {
        warn!("Unhandled rejection: {:?}", err);
        warp::reply::with_status(
            "Internal server error".into(),
            hyper::StatusCode::INTERNAL_SERVER_ERROR,
        )
    };

    let mut resp = reply.into_response();
    resp.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::header::HeaderValue::from_static("text/plain"),
    );
    Ok(resp)
}
