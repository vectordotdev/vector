use std::{
    collections::HashMap, convert::Infallible, fmt, net::SocketAddr, sync::Arc, time::Duration,
};

use serde_json::Value as JsonValue;

use bytes::Bytes;
use futures::{FutureExt, TryFutureExt};
use hyper::{Server, service::make_service_fn};
use tokio::net::TcpStream;
use tower::ServiceBuilder;
use tracing::Span;
use vector_lib::{
    EstimatedJsonEncodedSizeOf, TimeZone,
    config::SourceAcknowledgementsConfig,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, EventMetadata, VrlTarget},
};
use vrl::{
    compiler::{
        Program,
        runtime::{Runtime, Terminate},
    },
    value::Value,
};
use warp::{
    Filter,
    filters::{
        BoxedFilter,
        path::{FullPath, Tail},
    },
    http::{HeaderMap, StatusCode},
    reject::Rejection,
    reply::Reply,
};

use super::encoding::decompress_body;
use crate::{
    SourceSender,
    common::http::{ErrorMessage, server_auth::HttpServerAuthConfig},
    config::SourceContext,
    http::{KeepaliveConfig, MaxConnectionAgeLayer, build_http_trace_layer},
    internal_events::{
        HttpBadRequest, HttpBytesReceived, HttpEventsReceived, HttpInternalError, StreamClosedError,
    },
    sources::util::http::HttpMethod,
    tls::{MaybeTlsIncomingStream, MaybeTlsSettings, TlsEnableableConfig},
};

/// The decision returned by `build_vrl_response` after running the VRL `response_source` program.
///
/// - `Forward(response)` — the program returned normally. Send events to the sink and, once the
///   sink acknowledges them, send `response` back to the HTTP client.
/// - `Reject(response)` — the program called `abort`. Drop the decoded events without forwarding
///   them to the sink and send `response` back to the HTTP client immediately. Acknowledgements
///   are never involved, so the response can never be overridden by a sink failure.
enum VrlResponseDecision {
    Forward(warp::reply::Response),
    Reject(warp::reply::Response),
}

pub trait HttpSource: Clone + Send + Sync + 'static {
    /// Optional compiled VRL program used to generate the HTTP response body.
    /// The default returns `None`, meaning the source responds with the configured status code
    /// and an empty body. Override this in implementations that support `response_source`.
    fn response_source(&self) -> Option<Arc<Program>> {
        None
    }

    /// The HTTP status code returned to the client when the `response_source` VRL program calls
    /// `abort`. Signals that the request was intentionally rejected by the program before any
    /// events were forwarded. Override this in implementations that expose a configurable
    /// `reject_code`.
    fn reject_code(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }

    // This function can be defined to enrich events with additional HTTP
    // metadata. This function should be used rather than internal enrichment so
    // that accurate byte count metrics can be emitted.
    fn enrich_events(
        &self,
        _events: &mut [Event],
        _request_path: &str,
        _headers_config: &HeaderMap,
        _query_parameters: &HashMap<String, String>,
        _source_ip: Option<&SocketAddr>,
    ) {
    }

    fn build_events(
        &self,
        body: Bytes,
        header_map: &HeaderMap,
        query_parameters: &HashMap<String, String>,
        path: &str,
    ) -> Result<Vec<Event>, ErrorMessage>;

    fn decode(&self, encoding_header: Option<&str>, body: Bytes) -> Result<Bytes, ErrorMessage> {
        decompress_body(encoding_header, body)
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        self,
        address: SocketAddr,
        path: &str,
        method: HttpMethod,
        response_code: StatusCode,
        strict_path: bool,
        tls: Option<&TlsEnableableConfig>,
        auth: Option<&HttpServerAuthConfig>,
        cx: SourceContext,
        acknowledgements: SourceAcknowledgementsConfig,
        keepalive_settings: KeepaliveConfig,
    ) -> crate::Result<crate::sources::Source> {
        let tls = MaybeTlsSettings::from_config(tls, true)?;
        let protocol = tls.http_protocol_name();
        let auth_matcher = auth
            .map(|a| a.build(&cx.enrichment_tables, &cx.metrics_storage))
            .transpose()?;
        let path = path.to_owned();
        let acknowledgements = cx.do_acknowledgements(acknowledgements);
        let enable_source_ip = self.enable_source_ip();

        Ok(Box::pin(async move {
            let mut filter: BoxedFilter<()> = match method {
                HttpMethod::Head => warp::head().boxed(),
                HttpMethod::Get => warp::get().boxed(),
                HttpMethod::Put => warp::put().boxed(),
                HttpMethod::Post => warp::post().boxed(),
                HttpMethod::Patch => warp::patch().boxed(),
                HttpMethod::Delete => warp::delete().boxed(),
                HttpMethod::Options => warp::options().boxed(),
            };

            // https://github.com/rust-lang/rust-clippy/issues/8148
            #[allow(clippy::unnecessary_to_owned)]
            for s in path.split('/').filter(|&x| !x.is_empty()) {
                filter = filter.and(warp::path(s.to_string())).boxed()
            }
            let svc = filter
                .and(warp::path::tail())
                .and_then(move |tail: Tail| async move {
                    if !strict_path || tail.as_str().is_empty() {
                        Ok(())
                    } else {
                        emit!(HttpInternalError {
                            message: "Path not found."
                        });
                        Err(warp::reject::custom(ErrorMessage::new(
                            StatusCode::NOT_FOUND,
                            "Not found".to_string(),
                        )))
                    }
                })
                .untuple_one()
                .and(warp::path::full())
                .and(warp::header::optional::<String>("content-encoding"))
                .and(warp::header::headers_cloned())
                .and(warp::body::bytes())
                .and(warp::query::<HashMap<String, String>>())
                .and(warp::filters::ext::optional())
                .and_then(
                    move |path: FullPath,
                          encoding_header: Option<String>,
                          headers: HeaderMap,
                          body: Bytes,
                          query_parameters: HashMap<String, String>,
                          addr: Option<PeerAddr>| {
                        debug!(message = "Handling HTTP request.", headers = ?headers);
                        let http_path = path.as_str();
                        let events = auth_matcher
                            .as_ref()
                            .map_or(Ok(()), |a| {
                                a.handle_auth(
                                    addr.as_ref().map(|a| a.0).as_ref(),
                                    &headers,
                                    path.as_str(),
                                )
                            })
                            .and_then(|()| self.decode(encoding_header.as_deref(), body))
                            .and_then(|body| {
                                emit!(HttpBytesReceived {
                                    byte_size: body.len(),
                                    http_path,
                                    protocol,
                                });
                                self.build_events(body, &headers, &query_parameters, path.as_str())
                            })
                            .map(|mut events| {
                                emit!(HttpEventsReceived {
                                    count: events.len(),
                                    byte_size: events.estimated_json_encoded_size_of(),
                                    http_path,
                                    protocol,
                                });

                                self.enrich_events(
                                    &mut events,
                                    path.as_str(),
                                    &headers,
                                    &query_parameters,
                                    addr.and_then(|a| enable_source_ip.then_some(a))
                                        .map(|PeerAddr(inner_addr)| inner_addr)
                                        .as_ref(),
                                );

                                events
                            });

                        let response_source = self.response_source();
                        let reject_code = self.reject_code();
                        handle_request(
                            events,
                            acknowledgements,
                            response_code,
                            reject_code,
                            response_source,
                            cx.out.clone(),
                        )
                    },
                );

            let ping = warp::get().and(warp::path("ping")).map(|| "pong");
            let routes = svc.or(ping).recover(|r: Rejection| async move {
                if let Some(e_msg) = r.find::<ErrorMessage>() {
                    let json = warp::reply::json(e_msg);
                    Ok(warp::reply::with_status(json, e_msg.status_code()))
                } else {
                    //other internal error - will return 500 internal server error
                    emit!(HttpInternalError {
                        message: &format!("Internal error: {r:?}")
                    });
                    Err(r)
                }
            });

            let span = Span::current();
            let make_svc = make_service_fn(move |conn: &MaybeTlsIncomingStream<TcpStream>| {
                let remote_addr = conn.peer_addr();
                let svc = ServiceBuilder::new()
                    .layer(build_http_trace_layer(span.clone()))
                    .option_layer(keepalive_settings.max_connection_age_secs.map(|secs| {
                        MaxConnectionAgeLayer::new(
                            Duration::from_secs(secs),
                            keepalive_settings.max_connection_age_jitter_factor,
                            remote_addr,
                        )
                    }))
                    .map_request(move |mut request: hyper::Request<_>| {
                        request.extensions_mut().insert(PeerAddr::new(remote_addr));

                        request
                    })
                    .service(warp::service(routes.clone()));
                futures_util::future::ok::<_, Infallible>(svc)
            });

            info!(message = "Building HTTP server.", address = %address);

            let listener = tls.bind(&address).await.map_err(|err| {
                error!("An error occurred: {:?}.", err);
            })?;

            Server::builder(hyper::server::accept::from_stream(listener.accept_stream()))
                .serve(make_svc)
                .with_graceful_shutdown(cx.shutdown.map(|_| ()))
                .await
                .map_err(|err| {
                    error!("An error occurred: {:?}.", err);
                })?;

            Ok(())
        }))
    }

    fn enable_source_ip(&self) -> bool {
        false
    }
}

#[derive(Clone)]
#[repr(transparent)]
struct PeerAddr(SocketAddr);

impl PeerAddr {
    const fn new(addr: SocketAddr) -> Self {
        Self(addr)
    }
}

struct RejectShuttingDown;

impl fmt::Debug for RejectShuttingDown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("shutting down")
    }
}

impl warp::reject::Reject for RejectShuttingDown {}

async fn handle_request(
    events: Result<Vec<Event>, ErrorMessage>,
    acknowledgements: bool,
    response_code: StatusCode,
    reject_code: StatusCode,
    response_source: Option<Arc<Program>>,
    mut out: SourceSender,
) -> Result<warp::reply::Response, Rejection> {
    match events {
        Ok(mut events) => {
            let decision = match response_source {
                Some(ref program) => {
                    build_vrl_response(&events, program, response_code, reject_code)?
                }
                None => VrlResponseDecision::Forward(response_code.into_response()),
            };

            match decision {
                // The VRL program called `abort` — drop events, respond immediately.
                // Acknowledgements are not involved, so the response is never overridden.
                VrlResponseDecision::Reject(response) => Ok(response),

                // The VRL program returned normally — forward events to the sink and wait
                // for the acknowledgement before responding to the HTTP client.
                VrlResponseDecision::Forward(response) => {
                    let receiver = BatchNotifier::maybe_apply_to(acknowledgements, &mut events);
                    let count = events.len();
                    out.send_batch(events)
                        .map_err(|_| {
                            // can only fail if receiving end disconnected, so we are shutting down,
                            // probably not gracefully.
                            emit!(StreamClosedError { count });
                            warp::reject::custom(RejectShuttingDown)
                        })
                        .and_then(|_| handle_batch_status(response, receiver))
                        .await
                }
            }
        }
        Err(error) => {
            emit!(HttpBadRequest::new(error.code(), error.message()));
            Err(warp::reject::custom(error))
        }
    }
}

async fn handle_batch_status(
    success_response: warp::reply::Response,
    receiver: Option<BatchStatusReceiver>,
) -> Result<warp::reply::Response, Rejection> {
    match receiver {
        None => Ok(success_response),
        Some(receiver) => match receiver.await {
            BatchStatus::Delivered => Ok(success_response),
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

fn build_vrl_response(
    events: &[Event],
    program: &Program,
    default_status: StatusCode,
    reject_code: StatusCode,
) -> Result<VrlResponseDecision, Rejection> {
    let event_values: Vec<Value> = events
        .iter()
        .filter_map(|e| e.maybe_as_log())
        .map(|log| log.value().clone())
        .collect();

    let target_value = Value::Array(event_values);
    let mut target = VrlTarget::LogEvent(target_value, EventMetadata::default());

    match Runtime::default().resolve(&mut target, program, &TimeZone::default()) {
        // The program called `abort`. Suppress event forwarding and respond immediately.
        //
        // The abort message can be either:
        // - A plain string — used directly as the response body with `reject_code` as the status.
        // - A JSON-encoded object with the same shape as the normal return path:
        //   `{ "status": <integer>, "body": <string>, "headers": <object> }`.
        //   When a JSON object is provided, `status`, `body`, and `headers` are each optional;
        //   `status` defaults to `reject_code` if omitted.
        //
        // This lets the VRL program vary both the status code and body per `abort` call without
        // requiring any changes to the upstream VRL crate.
        Err(Terminate::Abort(err)) => {
            let message = match err {
                vrl::compiler::expression::ExpressionError::Abort { message, .. } => message,
                _ => None,
            };
            let response = match message {
                Some(msg) => match serde_json::from_str::<JsonValue>(&msg) {
                    Ok(JsonValue::Object(obj)) => build_response_from_json_obj(&obj, reject_code)?,
                    // Not a JSON object (plain string, or JSON but not an object) — use as-is.
                    _ => build_plain_reject_response(reject_code, msg)?,
                },
                None => warp::http::Response::builder()
                    .status(reject_code)
                    .body(hyper::Body::empty())
                    .map_err(|err| {
                        warp::reject::custom(ErrorMessage::new(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Failed to build reject response: {err}"),
                        ))
                    })?,
            };
            Ok(VrlResponseDecision::Reject(response))
        }
        Err(Terminate::Error(err)) => {
            emit!(HttpInternalError {
                message: &format!("VRL response program failed: {err}")
            });
            Err(warp::reject::custom(ErrorMessage::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("VRL response program failed: {err}"),
            )))
        }
        Ok(Value::Bytes(body)) => {
            let response = warp::http::Response::builder()
                .status(default_status)
                .body(hyper::Body::from(body))
                .map_err(|err| {
                    warp::reject::custom(ErrorMessage::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to build response: {err}"),
                    ))
                })?;
            Ok(VrlResponseDecision::Forward(response))
        }
        Ok(Value::Object(obj)) => {
            let response = build_response_from_vrl_obj(&obj, default_status)?;
            Ok(VrlResponseDecision::Forward(response))
        }
        Ok(other) => {
            emit!(HttpInternalError {
                message: "VRL response program returned unexpected type"
            });
            Err(warp::reject::custom(ErrorMessage::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("VRL response program must return a string or object, got: {other:?}"),
            )))
        }
    }
}

/// Builds an HTTP response from a VRL `Value::Object` returned by the `response_source` program.
/// Used by both the normal return path and the JSON-encoded abort path.
///
/// The object may contain:
/// - `status`  — integer HTTP status code; falls back to `default_status` if absent or invalid.
/// - `body`    — string response body; empty if absent.
/// - `headers` — object of string header name → string value pairs; ignored if absent.
fn build_response_from_vrl_obj(
    obj: &vrl::value::ObjectMap,
    default_status: StatusCode,
) -> Result<warp::reply::Response, Rejection> {
    let status = obj
        .get("status")
        .and_then(|v| v.as_integer())
        .and_then(|n| StatusCode::from_u16(n as u16).ok())
        .unwrap_or(default_status);

    let body = obj
        .get("body")
        .map(|v| match v {
            Value::Bytes(b) => hyper::Body::from(b.clone()),
            other => hyper::Body::from(other.to_string()),
        })
        .unwrap_or_else(hyper::Body::empty);

    let mut builder = warp::http::Response::builder().status(status);

    if let Some(Value::Object(headers)) = obj.get("headers") {
        for (k, v) in headers {
            if let Value::Bytes(v) = v {
                builder = builder.header(k.as_str(), v.as_ref());
            }
        }
    }

    builder.body(body).map_err(|err| {
        warp::reject::custom(ErrorMessage::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to build response: {err}"),
        ))
    })
}

/// Builds an HTTP response from a JSON object encoded in an `abort` message string.
///
/// The object may contain:
/// - `status`  — integer HTTP status code; falls back to `default_status` if absent or invalid.
/// - `body`    — string response body; empty if absent.
/// - `headers` — object of string header name → string value pairs; ignored if absent.
fn build_response_from_json_obj(
    obj: &serde_json::Map<String, JsonValue>,
    default_status: StatusCode,
) -> Result<warp::reply::Response, Rejection> {
    let status = obj
        .get("status")
        .and_then(|v| v.as_u64())
        .and_then(|n| StatusCode::from_u16(n as u16).ok())
        .unwrap_or(default_status);

    let body = obj
        .get("body")
        .and_then(|v| v.as_str())
        .map(|s| hyper::Body::from(s.to_owned()))
        .unwrap_or_else(hyper::Body::empty);

    let mut builder = warp::http::Response::builder().status(status);

    if let Some(JsonValue::Object(headers)) = obj.get("headers") {
        for (k, v) in headers {
            if let Some(v) = v.as_str() {
                builder = builder.header(k.as_str(), v);
            }
        }
    }

    builder.body(body).map_err(|err| {
        warp::reject::custom(ErrorMessage::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to build reject response: {err}"),
        ))
    })
}

/// Builds a plain-text reject response using `reject_code` as the status and `msg` as the body.
fn build_plain_reject_response(
    reject_code: StatusCode,
    msg: String,
) -> Result<warp::reply::Response, Rejection> {
    warp::http::Response::builder()
        .status(reject_code)
        .body(hyper::Body::from(msg))
        .map_err(|err| {
            warp::reject::custom(ErrorMessage::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build reject response: {err}"),
            ))
        })
}
