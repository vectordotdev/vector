use std::time::Duration;
use std::{convert::Infallible, net::SocketAddr};

use bytes::Bytes;
use futures_util::FutureExt;
use http::StatusCode;
use hyper::{service::make_service_fn, Server};
use prost::Message;
use snafu::Snafu;
use tokio::net::TcpStream;
use tower::ServiceBuilder;
use tracing::Span;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Registered,
};
use vector_lib::opentelemetry::proto::collector::{
    logs::v1::{ExportLogsServiceRequest, ExportLogsServiceResponse},
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

#[derive(Clone, Copy, Debug, Snafu)]
pub(crate) enum ApiError {
    ServerShutdown,
}

impl warp::reject::Reject for ApiError {}

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
        headers,
    );
    let trace_filters = build_warp_trace_filter(
        acknowledgements,
        out.clone(),
        bytes_received,
        events_received,
    );
    log_filters.or(trace_filters).unify().boxed()
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
        .and(warp::header::exact_ignore_case(
            "content-type",
            "application/x-protobuf",
        ))
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::header::headers_cloned())
        .and(warp::body::bytes())
        .and_then(
            move |encoding_header: Option<String>, headers_config: HeaderMap, body: Bytes| {
                let events = decode(encoding_header.as_deref(), body)
                    .and_then(|body| {
                        bytes_received.emit(ByteSize(body.len()));
                        decode_log_body(body, log_namespace, &events_received)
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
        .and(warp::header::exact_ignore_case(
            "content-type",
            "application/x-protobuf",
        ))
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::body::bytes())
        .and_then(move |encoding_header: Option<String>, body: Bytes| {
            let events = decode(encoding_header.as_deref(), body).and_then(|body| {
                bytes_received.emit(ByteSize(body.len()));
                decode_trace_body(body, &events_received)
            });

            handle_request(
                events,
                acknowledgements,
                out.clone(),
                super::TRACES,
                ExportTraceServiceResponse::default(),
            )
        })
        .boxed()
}

fn decode_trace_body(
    body: Bytes,
    events_received: &Registered<EventsReceived>,
) -> Result<Vec<Event>, ErrorMessage> {
    let request = ExportTraceServiceRequest::decode(body).map_err(|error| {
        ErrorMessage::new(
            StatusCode::BAD_REQUEST,
            format!("Could not decode request: {}", error),
        )
    })?;

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
) -> Result<Vec<Event>, ErrorMessage> {
    let request = ExportLogsServiceRequest::decode(body).map_err(|error| {
        ErrorMessage::new(
            StatusCode::BAD_REQUEST,
            format!("Could not decode request: {}", error),
        )
    })?;

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

async fn handle_request(
    events: Result<Vec<Event>, ErrorMessage>,
    acknowledgements: bool,
    mut out: SourceSender,
    output: &str,
    resp: impl Message,
) -> Result<Response, Rejection> {
    match events {
        Ok(mut events) => {
            let receiver = BatchNotifier::maybe_apply_to(acknowledgements, &mut events);
            let count = events.len();

            out.send_batch_named(output, events).await.map_err(|_| {
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
            message: format!("{:?}", err),
            ..Default::default()
        });

        Ok(warp::reply::with_status(
            reply,
            StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }
}
