use crate::{
    event::Event,
    internal_events::{HTTPBadRequest, HTTPEventsReceived},
    shutdown::ShutdownSignal,
    tls::{MaybeTlsSettings, TlsConfig},
    Pipeline,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{compat::Future01CompatExt, FutureExt, TryFutureExt};
use futures01::Sink;
use serde::Serialize;
use std::error::Error;
use std::fmt;
use std::net::SocketAddr;
use warp::{
    filters::BoxedFilter,
    http::{HeaderMap, StatusCode},
    reject::Rejection,
    Filter,
};

#[derive(Serialize, Debug)]
pub struct ErrorMessage {
    code: u16,
    message: String,
}
impl ErrorMessage {
    pub fn new(code: StatusCode, message: String) -> Self {
        ErrorMessage {
            code: code.as_u16(),
            message,
        }
    }
}
impl Error for ErrorMessage {}
impl fmt::Display for ErrorMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl warp::reject::Reject for ErrorMessage {}

struct RejectShuttingDown;
impl fmt::Debug for RejectShuttingDown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("shutting down")
    }
}
impl warp::reject::Reject for RejectShuttingDown {}

#[async_trait]
pub trait HttpSource: Clone + Send + Sync + 'static {
    fn build_event(&self, body: Bytes, header_map: HeaderMap) -> Result<Vec<Event>, ErrorMessage>;

    fn run(
        self,
        address: SocketAddr,
        path: &'static str,
        tls: &Option<TlsConfig>,
        out: Pipeline,
        shutdown: ShutdownSignal,
    ) -> crate::Result<crate::sources::Source> {
        let mut filter: BoxedFilter<()> = warp::post().boxed();
        if !path.is_empty() && path != "/" {
            for s in path.split('/') {
                filter = filter.and(warp::path(s)).boxed();
            }
        }
        let svc = filter
            .and(warp::path::end())
            .and(warp::header::headers_cloned())
            .and(warp::body::bytes())
            .and_then(move |headers: HeaderMap, body: Bytes| {
                info!("Handling HTTP request: {:?}", headers);

                let this = self.clone();
                let out = out.clone();

                async move {
                    let body_size = body.len();
                    match this.build_event(body, headers) {
                        Ok(events) => {
                            emit!(HTTPEventsReceived {
                                events_count: events.len(),
                                byte_size: body_size,
                            });
                            out.send_all(futures01::stream::iter_ok(events))
                                .compat()
                                .map_err(move |e: futures01::sync::mpsc::SendError<Event>| {
                                    // can only fail if receiving end disconnected, so we are shutting down,
                                    // probably not gracefully.
                                    error!("Failed to forward events, downstream is closed");
                                    error!("Tried to send the following event: {:?}", e);
                                    warp::reject::custom(RejectShuttingDown)
                                })
                                .map_ok(|_| warp::reply())
                                .await
                        }
                        Err(err) => {
                            emit!(HTTPBadRequest {
                                error_code: err.code,
                                error_message: err.message.as_str(),
                            });
                            Err(warp::reject::custom(err))
                        }
                    }
                }
            });

        let ping = warp::get().and(warp::path("ping")).map(|| "pong");
        let routes = svc.or(ping).recover(|r: Rejection| async move {
            if let Some(e_msg) = r.find::<ErrorMessage>() {
                let json = warp::reply::json(e_msg);
                Ok(warp::reply::with_status(
                    json,
                    StatusCode::from_u16(e_msg.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                ))
            } else {
                //other internal error - will return 500 internal server error
                Err(r)
            }
        });

        info!(message = "Building HTTP server", addr = %address);

        let tls = MaybeTlsSettings::from_config(tls, true)?;
        let fut = async move {
            let mut listener = tls.bind(&address).await.unwrap();
            let _ = warp::serve(routes)
                .serve_incoming_with_graceful_shutdown(
                    listener.incoming(),
                    shutdown.clone().compat().map(|_| ()),
                )
                .await;
            // We need to drop the last copy of ShutdownSignalToken only after server has shut down.
            drop(shutdown);
            Ok(())
        };
        Ok(Box::new(fut.boxed().compat()))
    }
}
