use bytes::Bytes;
use futures::{FutureExt, TryFutureExt};
use hyper::{service::make_service_fn, Server};
use serde_json::json;
use std::{
    collections::HashMap,
    convert::{Infallible, TryFrom},
    fmt,
    net::SocketAddr,
    time::Duration,
};
use tokio::net::TcpStream;
use tower::ServiceBuilder;
use tracing::Span;
use vector_lib::{
    config::SourceAcknowledgementsConfig,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    lookup::lookup_v2::OptionalTargetPath,
    EstimatedJsonEncodedSizeOf,
};
use warp::{
    filters::{
        path::{FullPath, Tail},
        BoxedFilter,
    },
    http::{HeaderMap, StatusCode},
    reject::Rejection,
    reply::Reply,
    Filter,
};

use crate::{
    config::SourceContext,
    http::{build_http_trace_layer, KeepaliveConfig, MaxConnectionAgeLayer},
    internal_events::{
        HttpBadRequest, HttpBytesReceived, HttpEventsReceived, HttpInternalError, StreamClosedError,
    },
    sources::util::http::HttpMethod,
    tls::{MaybeTlsIncomingStream, MaybeTlsSettings, TlsEnableableConfig},
    SourceSender,
};

use super::{
    auth::{HttpSourceAuth, HttpSourceAuthConfig},
    encoding::decode,
    error::ErrorMessage,
};

pub trait HttpSource: Clone + Send + Sync + 'static {
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
        decode(encoding_header, body)
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        self,
        address: SocketAddr,
        path: &str,
        method: HttpMethod,
        response_code: StatusCode,
        response_body_key: OptionalTargetPath,
        strict_path: bool,
        tls: &Option<TlsEnableableConfig>,
        auth: &Option<HttpSourceAuthConfig>,
        cx: SourceContext,
        acknowledgements: SourceAcknowledgementsConfig,
        keepalive_settings: KeepaliveConfig,
    ) -> crate::Result<crate::sources::Source> {
        let tls = MaybeTlsSettings::from_config(tls, true)?;
        let protocol = tls.http_protocol_name();
        let auth = HttpSourceAuth::try_from(auth.as_ref())?;
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
                .and(warp::header::optional::<String>("authorization"))
                .and(warp::header::optional::<String>("content-encoding"))
                .and(warp::header::headers_cloned())
                .and(warp::body::bytes())
                .and(warp::query::<HashMap<String, String>>())
                .and(warp::filters::ext::optional())
                .and_then(
                    move |path: FullPath,
                          auth_header,
                          encoding_header: Option<String>,
                          headers: HeaderMap,
                          body: Bytes,
                          query_parameters: HashMap<String, String>,
                          addr: Option<PeerAddr>| {
                        debug!(message = "Handling HTTP request.", headers = ?headers);
                        let http_path = path.as_str();

                        let events = auth
                            .is_valid(&auth_header)
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
                                    addr.map(|PeerAddr(inner_addr)| inner_addr).as_ref(),
                                );

                                events
                            });

                        handle_request(
                            events,
                            acknowledgements,
                            response_code,
                            response_body_key.clone(),
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
                        message: &format!("Internal error: {:?}", r)
                    });
                    Err(r)
                }
            });

            let span = Span::current();
            let make_svc = make_service_fn(move |conn: &MaybeTlsIncomingStream<TcpStream>| {
                let remote_addr = conn.peer_addr();
                let remote_addr_ref = enable_source_ip.then_some(remote_addr);
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
                        if let Some(remote_addr_inner) = remote_addr_ref.as_ref() {
                            request
                                .extensions_mut()
                                .insert(PeerAddr::new(*remote_addr_inner));
                        }

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
    response_body_key: OptionalTargetPath,
    mut out: SourceSender,
) -> Result<impl warp::Reply, Rejection> {
    match events {
        Ok(mut events) => {
            let mut response = response_code.into_response();

            if let Some(path) = &response_body_key.path {
                if let Some(first_event) = events.first() {
                    if let Some(body) = first_event.as_log().get(path.to_string().as_str()) {
                        response = warp::reply::with_status(
                            warp::reply::json(&json!(body)),
                            response_code,
                        )
                        .into_response();
                    } else {
                        return Err(warp::reject::custom(ErrorMessage::new(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Error generating response body".into(),
                        )));
                    }
                }
            }

            let count = events.len();
            let receiver = BatchNotifier::maybe_apply_to(acknowledgements, &mut events);
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
        Err(error) => {
            emit!(HttpBadRequest::new(error.code(), error.message()));
            Err(warp::reject::custom(error))
        }
    }
}

async fn handle_batch_status(
    success_response: impl warp::Reply,
    receiver: Option<BatchStatusReceiver>,
) -> Result<impl warp::Reply, Rejection> {
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
