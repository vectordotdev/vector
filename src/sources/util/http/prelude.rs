use std::{collections::HashMap, fmt, net::SocketAddr, sync::Arc, time::Duration};

use serde_json::Value as JsonValue;

use bytes::Bytes;
use futures::{FutureExt, TryFutureExt};
use tower::ServiceBuilder;
use tracing::Span;
use vector_lib::{
    EstimatedJsonEncodedSizeOf, TimeZone,
    config::{LogNamespace, SourceAcknowledgementsConfig},
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, EventMetadata, VrlTarget},
    internal_event::emit,
};
use vrl::{
    compiler::{
        Program,
        runtime::{Runtime, Terminate},
    },
    path::{PathPrefix, ValuePath as _},
    value::{ObjectMap, Value},
};
use warp::{
    Filter,
    filters::{
        BoxedFilter,
        path::{FullPath, Tail},
    },
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    reject::Rejection,
    reply::Reply,
};

use super::encoding::{capped_body, decompress_body};
use crate::{
    SourceSender,
    common::http::{ErrorMessage, server_auth::HttpServerAuthConfig},
    config::SourceContext,
    http::{KeepaliveConfig, MaxConnectionAgeLayer, build_http_trace_layer, serve},
    internal_events::{
        HttpBadRequest, HttpBytesReceived, HttpEventsReceived, HttpInternalError, StreamClosedError,
    },
    sources::util::http::HttpMethod,
    tls::{MaybeTlsSettings, TlsAcceptorReloader, TlsEnableableConfig, TlsError},
};

/// The decision returned by `build_vrl_response` after running the VRL `response_source` program.
///
/// - `Forward(response)`: the program returned normally. Send events to the sink and, once the
///   sink acknowledges them, send `response` back to the HTTP client.
/// - `Reject(response)`: the program called `abort`. Drop the decoded events without forwarding
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

    fn log_namespace(&self) -> LogNamespace;

    fn name() -> &'static str;

    ///
    /// # Errors
    ///
    /// Returns an error if the request body cannot be decoded into events.
    fn build_events(
        &self,
        body: Bytes,
        header_map: &HeaderMap,
        query_parameters: &HashMap<String, String>,
        path: &str,
    ) -> Result<Vec<Event>, ErrorMessage>;

    /// Injects `%field` enrichment from a `custom` auth VRL program into events.
    /// Both namespaces use insert-if-empty semantics so auth enrichment never
    /// overwrites built-in source metadata (`path`, `host`, `headers`, …) that
    /// `enrich_events` already populated.
    /// Vector namespace: inserted into event metadata under `http_server.<field>` for
    ///   all event types (Log, Metric, Trace).
    /// Legacy namespace: inserted into the Log event body only (Metric/Trace are skipped).
    fn inject_auth_enrichment(&self, events: &mut [Event], enrichment: ObjectMap) {
        for event in events.iter_mut() {
            match self.log_namespace() {
                LogNamespace::Vector => {
                    // metadata_mut() dispatches to Log, Metric, and Trace so every
                    // decoded event type receives the auth enrichment fields.
                    let meta = event.metadata_mut().value_mut();
                    let source_name = Self::name();
                    for (key, value) in &enrichment {
                        let key_str = key.as_str();
                        let name_part = vrl::path!(source_name);
                        let key_part = vrl::path!(key_str);
                        let full_path = name_part.concat(key_part);
                        if meta.get(full_path.clone()).is_none() {
                            meta.insert(full_path, value.clone());
                        }
                    }
                }
                LogNamespace::Legacy => {
                    // Legacy enrichment targets the event body; only Log events have one.
                    if let Event::Log(log) = event {
                        for (key, value) in &enrichment {
                            log.try_insert(
                                (PathPrefix::Event, vrl::path!(key.as_str())),
                                value.clone(),
                            );
                        }
                    } else if !enrichment.is_empty() {
                        warn!(
                            message = "Auth metadata enrichment is not supported for trace/metric events in the legacy namespace and will be dropped. \
                                       Switch this source to the Vector namespace (`log_namespace: true`) to enable it for all event types.",
                            fields = ?enrichment.keys().collect::<Vec<_>>(),
                        );
                    }
                }
            }
        }
    }

    ///
    /// # Errors
    ///
    /// Returns an error if the content encoding is unsupported or decompression fails.
    fn decode(&self, encoding_header: Option<&str>, body: Bytes) -> Result<Bytes, ErrorMessage> {
        decompress_body(encoding_header, body)
    }

    /// Serves the acceptor held by `tls_reloader` when set, so the caller can swap in new TLS
    /// material at runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if TLS or authentication cannot be configured, or if `tls_reloader` is set
    /// but TLS is disabled.
    #[allow(
        clippy::too_many_lines,
        reason = "Splitting this source routine is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
    )]
    #[allow(clippy::too_many_arguments)]
    fn run(
        self,
        address: SocketAddr,
        path: &str,
        method: HttpMethod,
        response_code: StatusCode,
        strict_path: bool,
        tls: Option<&TlsEnableableConfig>,
        tls_reloader: Option<TlsAcceptorReloader>,
        auth: Option<&HttpServerAuthConfig>,
        cx: SourceContext,
        acknowledgements: SourceAcknowledgementsConfig,
        keepalive_settings: KeepaliveConfig,
    ) -> crate::Result<crate::sources::Source> {
        let tls = MaybeTlsSettings::from_config(tls, true)?;
        // Binding would catch this too, but only once the source runs.
        if tls_reloader.is_some() && tls.is_raw() {
            return Err(TlsError::ReloaderWithoutTls.into());
        }
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

            for s in path.split('/').filter(|&x| !x.is_empty()) {
                filter = filter.and(warp::path(s.to_string())).boxed();
            }
            let body_filter = capped_body();

            let svc = filter
                .and(warp::path::tail())
                .and_then(move |tail: Tail| async move {
                    if !strict_path || tail.as_str().is_empty() {
                        Ok(())
                    } else {
                        emit(HttpInternalError {
                            message: "Path not found.",
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
                .and(body_filter)
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
                            .map_or(Ok(None), |a| {
                                a.handle_auth(
                                    addr.as_ref().map(|a| a.0).as_ref(),
                                    &headers,
                                    path.as_str(),
                                )
                            })
                            .and_then(|auth_enrichment| {
                                self.decode(encoding_header.as_deref(), body)
                                    .map(|body| (body, auth_enrichment))
                            })
                            .and_then(|(body, auth_enrichment)| {
                                emit(HttpBytesReceived {
                                    byte_size: body.len(),
                                    http_path,
                                    protocol,
                                });
                                self.build_events(body, &headers, &query_parameters, path.as_str())
                                    .map(|events| (events, auth_enrichment))
                            })
                            .map(|(mut events, auth_enrichment)| {
                                emit(HttpEventsReceived {
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

                                if let Some(enrichment) = auth_enrichment {
                                    self.inject_auth_enrichment(&mut events, enrichment);
                                }

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
                    emit(HttpInternalError {
                        message: &format!("Internal error: {r:?}"),
                    });
                    Err(r)
                }
            });

            let span = Span::current();

            info!(message = "Building HTTP server.", address = %address);

            let listener = tls
                .bind_reloadable(&address, tls_reloader)
                .await
                .inspect_err(|err| error!("An error occurred: {err:?}."))?
                .with_keepalive(keepalive_settings.tcp_keepalive);

            serve(
                listener.accept_stream(),
                cx.shutdown.map(|_| ()),
                move |conn| {
                    let remote_addr = conn.peer_addr();
                    ServiceBuilder::new()
                        .layer(build_http_trace_layer(span.clone()))
                        .option_layer(keepalive_settings.max_connection_age_secs.map(|secs| {
                            MaxConnectionAgeLayer::new(
                                Duration::from_secs(secs),
                                keepalive_settings.max_connection_age_jitter_factor,
                                remote_addr,
                            )
                        }))
                        .map_request(move |mut request: http::Request<_>| {
                            request.extensions_mut().insert(PeerAddr::new(remote_addr));

                            request
                        })
                        .service(warp::service(routes.clone()))
                },
            )
            .await
            .inspect_err(|err| error!("An error occurred: {err:?}."))?;

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
                // The VRL program called `abort`, so drop events and respond immediately.
                // Acknowledgements are not involved, so the response is never overridden.
                VrlResponseDecision::Reject(response) => Ok(response),

                // The VRL program returned normally, so forward events to the sink and wait
                // for the acknowledgement before responding to the HTTP client.
                VrlResponseDecision::Forward(response) => {
                    let receiver = BatchNotifier::maybe_apply_to(acknowledgements, &mut events);
                    let count = events.len();
                    out.send_batch(events)
                        .map_err(|_| {
                            // can only fail if receiving end disconnected, so we are shutting down,
                            // probably not gracefully.
                            emit(StreamClosedError { count });
                            warp::reject::custom(RejectShuttingDown)
                        })
                        .and_then(|()| handle_batch_status(response, receiver))
                        .await
                }
            }
        }
        Err(error) => {
            emit(HttpBadRequest::new(error.code(), error.message()));
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
        // - A plain string, used directly as the response body with `reject_code` as the status.
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
                    Ok(JsonValue::Object(obj)) if is_control_object(&obj) => {
                        build_response_from_json_obj(&obj, reject_code)?
                    }
                    // Not a JSON object, or a JSON object that has no control field with the type
                    // the builder consumes (e.g. `abort encode_json({"error": "bad"})` or
                    // `abort encode_json({"status": "error"})`), treat the whole message string as
                    // the response body so the content is not silently dropped.
                    _ => build_plain_reject_response(reject_code, msg)?,
                },
                None => warp::http::Response::builder()
                    .status(reject_code)
                    .body(Bytes::new().into())
                    .map_err(|err| {
                        warp::reject::custom(ErrorMessage::new(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Failed to build reject response: {err}"),
                        ))
                    })?,
            };
            Ok(VrlResponseDecision::Reject(response))
        }
        Err(err @ (Terminate::Error(_) | Terminate::Interrupted)) => {
            emit(HttpInternalError {
                message: &format!("VRL response program failed: {err}"),
            });
            Err(warp::reject::custom(ErrorMessage::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("VRL response program failed: {err}"),
            )))
        }
        Ok(body @ (Value::Bytes(_) | Value::String(_))) => {
            let response = warp::http::Response::builder()
                .status(default_status)
                .body(body.coerce_to_bytes().into())
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
            emit(HttpInternalError {
                message: "VRL response program returned unexpected type",
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
/// - `status`: integer HTTP status code; falls back to `default_status` if absent or invalid.
/// - `body`: string response body; empty if absent.
/// - `headers`: object of string header name to string value pairs; ignored if absent.
fn build_response_from_vrl_obj(
    obj: &vrl::value::ObjectMap,
    default_status: StatusCode,
) -> Result<warp::reply::Response, Rejection> {
    let status = obj
        .get("status")
        .and_then(Value::as_integer)
        .and_then(|n| u16::try_from(n).ok())
        .and_then(|n| StatusCode::from_u16(n).ok())
        .unwrap_or(default_status);

    let body = obj
        .get("body")
        .map(|v| match v {
            v @ (Value::Bytes(_) | Value::String(_)) => v.coerce_to_bytes(),
            other => Bytes::from(other.to_string()),
        })
        .unwrap_or_default();

    let mut builder = warp::http::Response::builder().status(status);

    if let Some(Value::Object(headers)) = obj.get("headers") {
        for (k, v) in headers {
            if matches!(v, Value::Bytes(_) | Value::String(_)) {
                insert_validated_header(&mut builder, k.as_str(), &v.coerce_to_bytes());
            }
        }
    }

    builder.body(body.into()).map_err(|err| {
        warp::reject::custom(ErrorMessage::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to build response: {err}"),
        ))
    })
}

/// Inserts a single response header after validating its name and value.
///
/// Header data can be built from arbitrary event content, so an invalid name or value (for
/// example a value containing CR/LF) is skipped with a warning rather than failing the whole
/// response. Validating up front and inserting via `headers_mut` avoids latching an error on the
/// builder, which would otherwise make `body()` fail and drop the already-accepted events.
fn insert_validated_header(builder: &mut warp::http::response::Builder, name: &str, value: &[u8]) {
    let header_name = match HeaderName::from_bytes(name.as_bytes()) {
        Ok(name) => name,
        Err(error) => {
            warn!(message = "Skipping invalid response header name.", header = %name, %error);
            return;
        }
    };
    let header_value = match HeaderValue::from_bytes(value) {
        Ok(value) => value,
        Err(error) => {
            warn!(message = "Skipping invalid response header value.", header = %name, %error);
            return;
        }
    };
    if let Some(headers) = builder.headers_mut() {
        headers.insert(header_name, header_value);
    }
}

/// Returns true when a JSON object from an `abort` message is a structured control payload, i.e.
/// it has at least one control field with the type the builder consumes (`status` integer, `body`
/// string, `headers` object). Objects whose control keys carry other types (e.g.
/// `{"status": "error"}`) are returned verbatim by the plain-body path instead of being dropped.
fn is_control_object(obj: &serde_json::Map<String, JsonValue>) -> bool {
    obj.get("status").is_some_and(JsonValue::is_u64)
        || obj.get("body").is_some_and(JsonValue::is_string)
        || obj.get("headers").is_some_and(JsonValue::is_object)
}

/// Builds an HTTP response from a JSON object encoded in an `abort` message string.
///
/// The object may contain:
/// - `status`: integer HTTP status code; falls back to `default_status` if absent or invalid.
/// - `body`: string response body; empty if absent.
/// - `headers`: object of string header name to string value pairs; ignored if absent.
fn build_response_from_json_obj(
    obj: &serde_json::Map<String, JsonValue>,
    default_status: StatusCode,
) -> Result<warp::reply::Response, Rejection> {
    let status = obj
        .get("status")
        .and_then(JsonValue::as_u64)
        .and_then(|n| u16::try_from(n).ok())
        .and_then(|n| StatusCode::from_u16(n).ok())
        .unwrap_or(default_status);

    let body = obj
        .get("body")
        .and_then(|v| v.as_str())
        .map(|s| Bytes::from(s.to_owned()))
        .unwrap_or_default();

    let mut builder = warp::http::Response::builder().status(status);

    if let Some(JsonValue::Object(headers)) = obj.get("headers") {
        for (k, v) in headers {
            if let Some(v) = v.as_str() {
                insert_validated_header(&mut builder, k.as_str(), v.as_bytes());
            }
        }
    }

    builder.body(body.into()).map_err(|err| {
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
        .body(msg.into())
        .map_err(|err| {
            warp::reject::custom(ErrorMessage::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build reject response: {err}"),
            ))
        })
}
