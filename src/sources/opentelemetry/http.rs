use std::net::SocketAddr;

use bytes::Bytes;
use futures_util::FutureExt;
use http::StatusCode;
use opentelemetry_proto::proto::collector::logs::v1::{
    ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use prost::Message;
use snafu::Snafu;
use tracing::Span;
use vector_common::internal_event::{
    ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Registered,
};
use vector_core::{
    config::LogNamespace,
    event::{BatchNotifier, BatchStatus},
    EstimatedJsonEncodedSizeOf,
};
use warp::{filters::BoxedFilter, reject::Rejection, reply::Response, Filter, Reply};

use crate::{
    event::Event,
    internal_events::{EventsReceived, StreamClosedError},
    shutdown::ShutdownSignal,
    sources::util::{decode, ErrorMessage},
    tls::MaybeTlsSettings,
    SourceSender,
};

use super::{reply::protobuf, status::Status};

#[derive(Clone, Copy, Debug, Snafu)]
pub(crate) enum ApiError {
    BadRequest,
    ServerShutdown,
}

impl warp::reject::Reject for ApiError {}

pub(crate) async fn run_http_server(
    address: SocketAddr,
    tls_settings: MaybeTlsSettings,
    filters: BoxedFilter<(Response,)>,
    shutdown: ShutdownSignal,
) -> crate::Result<()> {
    let span = Span::current();
    let listener = tls_settings.bind(&address).await?;
    let routes = filters
        .with(warp::trace(move |_info| span.clone()))
        .recover(handle_rejection);

    info!(message = "Building HTTP server.", address = %address);

    warp::serve(routes)
        .serve_incoming_with_graceful_shutdown(listener.accept_stream(), shutdown.map(|_| ()))
        .await;

    Ok(())
}

pub(crate) fn build_warp_filter(
    acknowledgements: bool,
    log_namespace: LogNamespace,
    out: SourceSender,
    bytes_received: Registered<BytesReceived>,
    events_received: Registered<EventsReceived>,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(warp::path!("v1" / "logs"))
        .and(warp::header::exact_ignore_case(
            "content-type",
            "application/x-protobuf",
        ))
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::body::bytes())
        .and_then(move |encoding_header: Option<String>, body: Bytes| {
            let events = decode(&encoding_header, body).and_then(|body| {
                bytes_received.emit(ByteSize(body.len()));
                decode_body(body, log_namespace, &events_received)
            });

            handle_request(events, acknowledgements, out.clone(), super::LOGS)
        })
        .boxed()
}

fn decode_body(
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
) -> Result<Response, Rejection> {
    match events {
        Ok(mut events) => {
            let receiver = BatchNotifier::maybe_apply_to(acknowledgements, &mut events);
            let count = events.len();

            out.send_batch_named(output, events)
                .await
                .map_err(move |error| {
                    emit!(StreamClosedError { error, count });
                    warp::reject::custom(ApiError::ServerShutdown)
                })?;

            match receiver {
                None => Ok(protobuf(ExportLogsServiceResponse {}).into_response()),
                Some(receiver) => match receiver.await {
                    BatchStatus::Delivered => {
                        Ok(protobuf(ExportLogsServiceResponse {}).into_response())
                    }
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
