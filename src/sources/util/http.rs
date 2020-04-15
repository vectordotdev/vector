use crate::event::Event;
use crate::{
    shutdown::ShutdownSignal,
    tls::{MaybeTlsSettings, TlsConfig},
};
use futures01::{sync::mpsc, Future, IntoFuture, Sink};
use serde::Serialize;
use std::error::Error;
use std::fmt::{self, Display};
use std::net::SocketAddr;
use warp::filters::{body::FullBody, BoxedFilter};
use warp::http::{HeaderMap, StatusCode};
use warp::{Filter, Rejection};

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
impl Display for ErrorMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

pub trait HttpSource: Clone + Send + Sync + 'static {
    fn build_event(
        &self,
        body: FullBody,
        header_map: HeaderMap,
    ) -> Result<Vec<Event>, ErrorMessage>;

    fn run(
        self,
        address: SocketAddr,
        path: &'static str,
        tls: &Option<TlsConfig>,
        out: mpsc::Sender<Event>,
        shutdown: ShutdownSignal,
    ) -> crate::Result<crate::sources::Source> {
        let mut filter: BoxedFilter<()> = warp::post2().boxed();
        if !path.is_empty() && path != "/" {
            for s in path.split('/') {
                filter = filter.and(warp::path(s)).boxed();
            }
        }
        let svc = filter
            .and(warp::path::end())
            .and(warp::header::headers_cloned())
            .and(warp::body::concat())
            .and_then(move |headers: HeaderMap, body| {
                let out = out.clone();
                info!("Handling http request: {:?}", headers);

                self.build_event(body, headers)
                    .map_err(warp::reject::custom)
                    .into_future()
                    .and_then(|events| {
                        out.send_all(futures01::stream::iter_ok(events)).map_err(
                            move |e: mpsc::SendError<Event>| {
                                // can only fail if receiving end disconnected, so we are shuting down,
                                // probably not gracefully.
                                error!("Failed to forward events, downstream is closed");
                                error!("Tried to send the following event: {:?}", e);

                                warp::reject::custom("shutting down")
                            },
                        )
                    })
                    .map(|_| warp::reply())
            });

        let ping = warp::get2().and(warp::path("ping")).map(|| "pong");
        let routes = svc.or(ping).recover(|r: Rejection| {
            if let Some(e_msg) = r.find_cause::<ErrorMessage>() {
                let json = warp::reply::json(e_msg);
                Ok(warp::reply::with_status(
                    json,
                    StatusCode::from_u16(e_msg.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                ))
            } else {
                //other internal error - will return 500 internal server error
                Err(r)
            }
            .into_future()
        });

        info!(message = "building http server", addr = %address);

        let tls = MaybeTlsSettings::from_config(tls, true)?;
        let incoming = tls.bind(&address)?.incoming();

        let server = warp::serve(routes)
            .serve_incoming_with_graceful_shutdown(incoming, shutdown.clone().map(|_| ()));

        // We need to drop the last copy of ShutdownSignalToken only after server has shut down.
        Ok(Box::new(server.map(|_| drop(shutdown))))
    }
}
