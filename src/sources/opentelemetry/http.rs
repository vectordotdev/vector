use std::time::Duration;
use std::{convert::Infallible, net::SocketAddr};

use bytes::{Buf, Bytes};
use futures_util::FutureExt;
use http::StatusCode;
use hyper::{service::make_service_fn, Server};
use snafu::Snafu;
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

#[derive(Clone, Debug, Snafu)]
pub(crate) enum ApiError {
    ContentType { content_type: String },
}

impl warp::reject::Reject for ApiError {}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
enum ContentType {
    Protobuf,
    Json,
}

impl TryFrom<String> for ContentType {
    type Error = ();
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "application/x-protobuf" => Ok(ContentType::Protobuf),
            "application/json" => Ok(ContentType::Json),
            _ => Err(()),
        }
    }
}

pub(crate) async fn run_http_server(
    address: SocketAddr,
    tls_settings: MaybeTlsSettings,
    filters: BoxedFilter<(Response,)>,
    shutdown: ShutdownSignal,
    keepalive_settings: KeepaliveConfig,
) -> crate::Result<()> {
    let listener = tls_settings.bind(&address).await?;
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

fn extract_content_type() -> impl warp::Filter<Extract = (ContentType,), Error = Rejection> + Copy {
    warp::header::<String>("content-type").and_then(|content_type: String| async move {
        content_type
            .clone()
            .try_into()
            .map_err(|_| warp::reject::custom(ApiError::ContentType { content_type }))
    })
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
            move |content_type: ContentType,
                  encoding_header: Option<String>,
                  headers_config: HeaderMap,
                  body: Bytes| {
                let events = decode(encoding_header.as_deref(), body)
                    .and_then(|body| {
                        bytes_received.emit(ByteSize(body.len()));
                        decode_log_body(body, log_namespace, &events_received, content_type)
                    })
                    .map(|mut events| {
                        enrich_events(&mut events, &headers, &headers_config, log_namespace);
                        events
                    });

                handle_request(
                    events,
                    acknowledgements,
                    out.clone(),
                    super::LOGS,
                    ExportLogsServiceResponse::default(),
                    content_type,
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
            move |content_type: ContentType, encoding_header: Option<String>, body: Bytes| {
                let events = decode(encoding_header.as_deref(), body).and_then(|body| {
                    bytes_received.emit(ByteSize(body.len()));
                    decode_metrics_body(body, &events_received, content_type)
                });

                handle_request(
                    events,
                    acknowledgements,
                    out.clone(),
                    super::METRICS,
                    ExportMetricsServiceResponse::default(),
                    content_type,
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
            move |content_type: ContentType, encoding_header: Option<String>, body: Bytes| {
                let events = decode(encoding_header.as_deref(), body).and_then(|body| {
                    bytes_received.emit(ByteSize(body.len()));
                    decode_trace_body(body, &events_received, content_type)
                });

                handle_request(
                    events,
                    acknowledgements,
                    out.clone(),
                    super::TRACES,
                    ExportTraceServiceResponse::default(),
                    content_type,
                )
            },
        )
        .boxed()
}

fn decode_message<'a, T: prost::Message + std::default::Default + serde::de::DeserializeOwned>(
    content_type: ContentType,
    body: Bytes,
) -> Result<T, ErrorMessage> {
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

fn decode_trace_body(
    body: Bytes,
    events_received: &Registered<EventsReceived>,
    content_type: ContentType,
) -> Result<Vec<Event>, ErrorMessage> {
    let request = decode_message::<ExportTraceServiceRequest>(content_type, body)?;

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
    let request = decode_message::<ExportLogsServiceRequest>(content_type, body)?;

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
    let request = decode_message::<ExportMetricsServiceRequest>(content_type, body)?;

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

fn serialize_response<T: serde::Serialize + prost::Message>(
    content_type: ContentType,
    status_code: hyper::StatusCode,
    payload: T,
) -> Result<Response, Rejection> {
    match content_type {
        ContentType::Json => {
            let mut resp = warp::reply::json(&payload).into_response();
            *resp.status_mut() = status_code;
            Ok(resp)
        }
        ContentType::Protobuf => Ok(protobuf(status_code, payload).into_response()),
    }
}

async fn handle_request<T: prost::Message + std::default::Default + serde::Serialize>(
    events: Result<Vec<Event>, ErrorMessage>,
    acknowledgements: bool,
    mut out: SourceSender,
    output: &str,
    resp: T,
    content_type: ContentType,
) -> Result<Response, Rejection> {
    match events {
        Ok(mut events) => {
            let receiver = BatchNotifier::maybe_apply_to(acknowledgements, &mut events);
            let count = events.len();

            if let Err(_) = out.send_batch_named(output, events).await {
                emit!(StreamClosedError { count });
                return serialize_response(
                    content_type,
                    hyper::StatusCode::SERVICE_UNAVAILABLE,
                    Status {
                        code: 14,
                        message: "Vector is shutting down".into(),
                        ..Default::default()
                    },
                );
            }

            match receiver {
                None => serialize_response(content_type, hyper::StatusCode::OK, resp),
                Some(receiver) => match receiver.await {
                    BatchStatus::Delivered => {
                        serialize_response(content_type, hyper::StatusCode::OK, resp)
                    }
                    BatchStatus::Errored => serialize_response(
                        content_type,
                        hyper::StatusCode::OK,
                        Status {
                            code: 2, // UNKNOWN - OTLP doesn't require use of status.code, but we can't encode a None here
                            message: "Error delivering contents to sink".into(),
                            ..Default::default()
                        },
                    ),
                    BatchStatus::Rejected => serialize_response(
                        content_type,
                        hyper::StatusCode::OK,
                        Status {
                            code: 2, // UNKNOWN - OTLP doesn't require use of status.code, but we can't encode a None here
                            message: "Contents failed to deliver to sink".into(),
                            ..Default::default()
                        },
                    ),
                },
            }
        }
        Err(err) => serialize_response(
            content_type,
            hyper::StatusCode::BAD_REQUEST,
            Status {
                code: 2,
                message: format!("Unable to read events: {err}").into(),
                ..Default::default()
            },
        ),
    }
}

async fn handle_rejection(err: Rejection) -> Result<impl Reply, std::convert::Infallible> {
    if let Some(err_msg) = err.find::<ApiError>() {
        match err_msg {
            ApiError::ContentType { content_type } => Ok(warp::reply::with_status(
                format!("Invalid Content-Type header value: {content_type}"),
                hyper::StatusCode::BAD_REQUEST,
            )),
            ApiError::ServerShutdown => Ok(warp::reply::with_status(
                "server down".to_string(),
                hyper::StatusCode::BAD_REQUEST,
            )),
        }
    } else {
        Ok(warp::reply::with_status(
            "Unknown route".to_string(),
            hyper::StatusCode::NOT_FOUND,
        ))
    }
}
