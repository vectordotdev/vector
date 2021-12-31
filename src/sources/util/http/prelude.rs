use std::{collections::HashMap, convert::TryFrom, fmt, net::SocketAddr};

use async_trait::async_trait;
use bytes::Bytes;
use futures::{FutureExt, SinkExt, StreamExt, TryFutureExt};
use vector_core::{
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    ByteSizeOf,
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

use super::{
    auth::{HttpSourceAuth, HttpSourceAuthConfig},
    encoding::decode,
    error::ErrorMessage,
};
use crate::{
    config::{AcknowledgementsConfig, SourceContext},
    internal_events::{HttpBadRequest, HttpBytesReceived, HttpEventsReceived},
    tls::{MaybeTlsSettings, TlsConfig},
    Pipeline,
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

    fn run(
        self,
        address: SocketAddr,
        path: &str,
        strict_path: bool,
        tls: &Option<TlsConfig>,
        auth: &Option<HttpSourceAuthConfig>,
        cx: SourceContext,
        acknowledgements: AcknowledgementsConfig,
    ) -> crate::Result<crate::sources::Source> {
        let tls = MaybeTlsSettings::from_config(tls, true)?;
        let protocol = tls.http_protocol_name();
        let auth = HttpSourceAuth::try_from(auth.as_ref())?;
        let path = path.to_owned();
        Ok(Box::pin(async move {
            let span = crate::trace::current_span();
            let mut filter: BoxedFilter<()> = warp::post().boxed();
            for s in path.split('/').filter(|&x| !x.is_empty()) {
                filter = filter.and(warp::path(s.to_string())).boxed()
            }
            let svc = filter
                .and(warp::path::tail())
                .and_then(move |tail: Tail| async move {
                    if !strict_path || tail.as_str().is_empty() {
                        Ok(())
                    } else {
                        debug!(message = "Path rejected.");
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
                        emit!(&HttpBytesReceived {
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
                                emit!(&HttpEventsReceived {
                                    count: events.len(),
                                    byte_size: events.size_of(),
                                    http_path,
                                    protocol,
                                });
                                events
                            });

                        handle_request(events, acknowledgements.enabled, cx.out.clone())
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
                    Err(r)
                }
            });

            info!(message = "Building HTTP server.", address = %address);

            let listener = tls.bind(&address).await.unwrap();
            warp::serve(routes)
                .serve_incoming_with_graceful_shutdown(
                    listener.accept_stream(),
                    cx.shutdown.map(|_| ()),
                )
                .await;
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
    mut out: Pipeline,
) -> Result<impl warp::Reply, Rejection> {
    match events {
        Ok(mut events) => {
            let receiver = BatchNotifier::maybe_apply_to_events(acknowledgements, &mut events);

            out.send_all(&mut futures::stream::iter(events).map(Ok))
                .map_err(move |error: crate::pipeline::ClosedError| {
                    // can only fail if receiving end disconnected, so we are shutting down,
                    // probably not gracefully.
                    error!(message = "Failed to forward events, downstream is closed.");
                    error!(message = "Tried to send the following event.", %error);
                    warp::reject::custom(RejectShuttingDown)
                })
                .and_then(|_| handle_batch_status(receiver))
                .await
        }
        Err(error) => {
            emit!(&HttpBadRequest {
                error_code: error.code(),
                error_message: error.message(),
            });
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
