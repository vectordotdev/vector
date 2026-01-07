use std::{convert::Infallible, net::SocketAddr, time::Duration};

use bytes::Bytes;
use futures_util::FutureExt;
use http::StatusCode;
use hyper::{Server, service::make_service_fn};
use prost::Message;
use snafu::Snafu;
use tokio::net::TcpStream;
use tower::ServiceBuilder;
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
        util::{add_headers, decompress_body},
    },
    tls::MaybeTlsSettings,
};

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
        out.clone(),
        bytes_received.clone(),
        events_received.clone(),
        metrics_deserializer,
    );
    let trace_filters = build_warp_trace_filter(
        acknowledgements,
        out.clone(),
        bytes_received,
        events_received,
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

fn parse_with_deserializer(
    deserializer: &OtlpDeserializer,
    body: Bytes,
    log_namespace: LogNamespace,
) -> Result<Vec<Event>, ErrorMessage> {
    deserializer
        .parse(body, log_namespace)
        .map(|r| r.into_vec())
        .map_err(emit_decode_error)
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
        + Fn(Option<String>, HeaderMap, Bytes) -> Result<Vec<Event>, ErrorMessage>,
{
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
        .and(warp::body::bytes())
        .and_then(
            move |encoding_header: Option<String>, headers: HeaderMap, body: Bytes| {
                let events = make_events(encoding_header, headers, body);
                handle_request(
                    events,
                    acknowledgements,
                    out.clone(),
                    telemetry_type,
                    Resp::default(),
                )
            },
        )
        .boxed()
}

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
                    parse_with_deserializer(d, decoded_body, log_namespace)
                } else {
                    decode_log_body(decoded_body, log_namespace, &events_received)
                }
                .map(|mut events| {
                    enrich_events(&mut events, &headers_cfg, &headers, log_namespace);
                    events
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
fn build_warp_metrics_filter(
    acknowledgements: bool,
    source_sender: SourceSender,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
    deserializer: Option<OtlpDeserializer>,
) -> BoxedFilter<(Response,)> {
    let make_events = move |encoding_header: Option<String>, _headers: HeaderMap, body: Bytes| {
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
                    parse_with_deserializer(d, decoded_body, LogNamespace::default())
                } else {
                    decode_metrics_body(decoded_body, &events_received)
                }
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
    deserializer: Option<OtlpDeserializer>,
) -> BoxedFilter<(Response,)> {
    let make_events = move |encoding_header: Option<String>, _headers: HeaderMap, body: Bytes| {
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
                    parse_with_deserializer(d, decoded_body, LogNamespace::default())
                } else {
                    decode_trace_body(decoded_body, &events_received)
                }
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
) -> Result<Vec<Event>, ErrorMessage> {
    let request = ExportTraceServiceRequest::decode(body).map_err(emit_decode_error)?;

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
    let request = ExportLogsServiceRequest::decode(body).map_err(emit_decode_error)?;

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
) -> Result<Vec<Event>, ErrorMessage> {
    let request = ExportMetricsServiceRequest::decode(body).map_err(emit_decode_error)?;

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
            message: format!("{err:?}"),
            ..Default::default()
        });

        Ok(warp::reply::with_status(
            reply,
            StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }
}
