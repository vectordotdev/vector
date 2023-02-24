use std::{collections::HashMap, convert::TryFrom, fmt, net::SocketAddr};

use async_trait::async_trait;
use bytes::Bytes;
use futures::{FutureExt, TryFutureExt};
use tracing::Span;
use vector_core::{
    config::SourceAcknowledgementsConfig,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    EstimatedJsonEncodedSizeOf,
};
use warp::{
    filters::{
        path::{FullPath, Tail},
        BoxedFilter,
    },
    http::{HeaderMap, StatusCode},
    reject::Rejection,
    Filter,
};

use crate::{
    config::SourceContext,
    internal_events::{
        HttpBadRequest, HttpBytesReceived, HttpEventsReceived, HttpInternalError, StreamClosedError,
    },
    sources::util::http::HttpMethod,
    tls::{MaybeTlsSettings, TlsEnableableConfig},
    SourceSender,
};

use super::{
    auth::{HttpSourceAuth, HttpSourceAuthConfig},
    encoding::decode,
    error::ErrorMessage,
};

#[async_trait]
pub trait HttpSource: Clone + Send + Sync + 'static {
    fn build_events(
        &self,
        body: Bytes,
        header_map: HeaderMap,
        query_parameters: HashMap<String, String>,
        path: &str,
    ) -> Result<Vec<Event>, ErrorMessage>;

    #[allow(clippy::too_many_arguments)]
    fn run(
        self,
        address: SocketAddr,
        path: &str,
        method: HttpMethod,
        strict_path: bool,
        tls: &Option<TlsEnableableConfig>,
        auth: &Option<HttpSourceAuthConfig>,
        cx: SourceContext,
        acknowledgements: SourceAcknowledgementsConfig,
    ) -> crate::Result<crate::sources::Source> {
        let tls = MaybeTlsSettings::from_config(tls, true)?;
        let protocol = tls.http_protocol_name();
        let auth = HttpSourceAuth::try_from(auth.as_ref())?;
        let path = path.to_owned();
        let acknowledgements = cx.do_acknowledgements(acknowledgements);
        Ok(Box::pin(async move {
            let span = Span::current();
            let mut filter: BoxedFilter<()> = match method {
                HttpMethod::Head => warp::head().boxed(),
                HttpMethod::Get => warp::get().boxed(),
                HttpMethod::Put => warp::put().boxed(),
                HttpMethod::Post => warp::post().boxed(),
                HttpMethod::Patch => warp::patch().boxed(),
                HttpMethod::Delete => warp::delete().boxed(),
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
                .and_then(
                    move |path: FullPath,
                          auth_header,
                          encoding_header,
                          headers: HeaderMap,
                          body: Bytes,
                          query_parameters: HashMap<String, String>| {
                        debug!(message = "Handling HTTP request.", headers = ?headers);
                        let http_path = path.as_str();
                        emit!(HttpBytesReceived {
                            byte_size: body.len(),
                            http_path,
                            protocol,
                        });

                        let events = auth
                            .is_valid(&auth_header)
                            .and_then(|()| decode(&encoding_header, body))
                            .and_then(|body| {
                                self.build_events(body, headers, query_parameters, path.as_str())
                            })
                            .map(|events| {
                                emit!(HttpEventsReceived {
                                    count: events.len(),
                                    byte_size: events.estimated_json_encoded_size_of(),
                                    http_path,
                                    protocol,
                                });
                                events
                            });

                        handle_request(events, acknowledgements, cx.out.clone())
                    },
                )
                .with(warp::trace(move |_info| span.clone()));

            let ping = warp::get().and(warp::path("ping")).map(|| "pong");
            let routes = svc.or(ping).recover(|r: Rejection| async move {
                if let Some(e_msg) = r.find::<ErrorMessage>() {
                    let json = warp::reply::json(e_msg);
                    Ok(warp::reply::with_status(json, e_msg.status_code()))
                } else {
                    //other internal error - will return 500 internal server error
                    emit!(HttpInternalError {
                        message: "Internal error."
                    });
                    Err(r)
                }
            });

            info!(message = "Building HTTP server.", address = %address);

            match tls.bind(&address).await {
                Ok(listener) => {
                    warp::serve(routes)
                        .serve_incoming_with_graceful_shutdown(
                            listener.accept_stream(),
                            cx.shutdown.map(|_| ()),
                        )
                        .await;
                }
                Err(error) => {
                    error!("An error occurred: {:?}.", error);
                    return Err(());
                }
            }
            Ok(())
        }))
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
    mut out: SourceSender,
) -> Result<impl warp::Reply, Rejection> {
    match events {
        Ok(mut events) => {
            let receiver = BatchNotifier::maybe_apply_to(acknowledgements, &mut events);

            let count = events.len();
            out.send_batch(events)
                .map_err(move |error: crate::source_sender::ClosedError| {
                    // can only fail if receiving end disconnected, so we are shutting down,
                    // probably not gracefully.
                    emit!(StreamClosedError { error, count });
                    warp::reject::custom(RejectShuttingDown)
                })
                .and_then(|_| handle_batch_status(receiver))
                .await
        }
        Err(error) => {
            emit!(HttpBadRequest::new(error.code(), error.message()));
            Err(warp::reject::custom(error))
        }
    }
}

async fn handle_batch_status(
    receiver: Option<BatchStatusReceiver>,
) -> Result<impl warp::Reply, Rejection> {
    match receiver {
        None => Ok(warp::reply()),
        Some(receiver) => match receiver.await {
            BatchStatus::Delivered => Ok(warp::reply()),
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
