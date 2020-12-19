use crate::{
    event::Event,
    internal_events::{HTTPBadRequest, HTTPEventsReceived},
    shutdown::ShutdownSignal,
    tls::{MaybeTlsSettings, TlsConfig},
    Pipeline,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{FutureExt, SinkExt, StreamExt, TryFutureExt};
use headers::{Authorization, HeaderMapExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::TryFrom, error::Error, fmt, net::SocketAddr};
use tracing_futures::Instrument;
use warp::{
    filters::BoxedFilter,
    http::{HeaderMap, StatusCode},
    reject::Rejection,
    Filter,
};

#[cfg(any(feature = "sources-http", feature = "sources-heroku_logs"))]
pub(crate) fn add_query_parameters(
    mut events: Vec<Event>,
    query_parameters_config: &[String],
    query_parameters: HashMap<String, String>,
) -> Vec<Event> {
    for query_parameter_name in query_parameters_config {
        let value = query_parameters.get(query_parameter_name);
        for event in events.iter_mut() {
            event.as_mut_log().insert(
                query_parameter_name as &str,
                crate::event::Value::from(value.map(String::to_owned)),
            );
        }
    }

    events
}

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

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct HttpSourceAuthConfig {
    pub username: String,
    pub password: String,
}

impl TryFrom<Option<&HttpSourceAuthConfig>> for HttpSourceAuth {
    type Error = String;

    fn try_from(auth: Option<&HttpSourceAuthConfig>) -> Result<Self, Self::Error> {
        match auth {
            Some(auth) => {
                let mut headers = HeaderMap::new();
                headers.typed_insert(Authorization::basic(&auth.username, &auth.password));
                match headers.get("authorization") {
                    Some(value) => {
                        let token = value
                            .to_str()
                            .map_err(|error| format!("Failed stringify HeaderValue: {:?}", error))?
                            .to_owned();
                        Ok(HttpSourceAuth { token: Some(token) })
                    }
                    None => Err("Authorization headers wasn't generated".to_owned()),
                }
            }
            None => Ok(HttpSourceAuth { token: None }),
        }
    }
}

#[derive(Debug, Clone)]
struct HttpSourceAuth {
    pub token: Option<String>,
}

impl HttpSourceAuth {
    pub fn is_valid(&self, header: &Option<String>) -> Result<(), ErrorMessage> {
        match (&self.token, header) {
            (Some(token1), Some(token2)) => {
                if token1 == token2 {
                    Ok(())
                } else {
                    Err(ErrorMessage::new(
                        StatusCode::UNAUTHORIZED,
                        "Invalid username/password".to_owned(),
                    ))
                }
            }
            (Some(_), None) => Err(ErrorMessage::new(
                StatusCode::UNAUTHORIZED,
                "No authorization header".to_owned(),
            )),
            (None, _) => Ok(()),
        }
    }
}

#[async_trait]
pub trait HttpSource: Clone + Send + Sync + 'static {
    fn build_event(
        &self,
        body: Bytes,
        header_map: HeaderMap,
        query_parameters: HashMap<String, String>,
    ) -> Result<Vec<Event>, ErrorMessage>;

    fn run(
        self,
        address: SocketAddr,
        path: &'static str,
        tls: &Option<TlsConfig>,
        auth: &Option<HttpSourceAuthConfig>,
        out: Pipeline,
        shutdown: ShutdownSignal,
    ) -> crate::Result<crate::sources::Source> {
        let tls = MaybeTlsSettings::from_config(tls, true)?;
        let auth = HttpSourceAuth::try_from(auth.as_ref())?;
        Ok(Box::pin(async move {
            let span = crate::trace::current_span();

            let mut filter: BoxedFilter<()> = warp::post().boxed();
            if !path.is_empty() && path != "/" {
                for s in path.split('/') {
                    filter = filter.and(warp::path(s)).boxed();
                }
            }
            let svc = filter
                .and(warp::path::end())
                .and(warp::header::optional::<String>("authorization"))
                .and(warp::header::headers_cloned())
                .and(warp::body::bytes())
                .and(warp::query::<HashMap<String, String>>())
                .and_then(
                    move |auth_header,
                          headers: HeaderMap,
                          body: Bytes,
                          query_parameters: HashMap<String, String>| {
                        info!(message = "Handling HTTP request.", headers = ?headers, rate_limit_secs = 30);

                        let mut out = out.clone();

                        let body_size = body.len();
                        let events = match auth.is_valid(&auth_header) {
                            Ok(()) => self.build_event(body, headers, query_parameters),
                            Err(err) => Err(err),
                        };

                        async move {
                            match events {
                                Ok(events) => {
                                    emit!(HTTPEventsReceived {
                                        events_count: events.len(),
                                        byte_size: body_size,
                                    });
                                    out.send_all(&mut futures::stream::iter(events).map(Ok))
                                        .map_err(move |error: crate::pipeline::ClosedError| {
                                            // can only fail if receiving end disconnected, so we are shutting down,
                                            // probably not gracefully.
                                            error!(message = "Failed to forward events, downstream is closed.");
                                            error!(message = "Tried to send the following event.", %error);
                                            warp::reject::custom(RejectShuttingDown)
                                        })
                                        .map_ok(|_| warp::reply())
                                        .await
                                }
                                Err(error) => {
                                    emit!(HTTPBadRequest {
                                        error_code: error.code,
                                        error_message: error.message.as_str(),
                                    });
                                    Err(warp::reject::custom(error))
                                }
                            }
                        }
                        .instrument(span.clone())
                    },
                );

            let ping = warp::get().and(warp::path("ping")).map(|| "pong");
            let routes = svc.or(ping).recover(|r: Rejection| async move {
                if let Some(e_msg) = r.find::<ErrorMessage>() {
                    let json = warp::reply::json(e_msg);
                    Ok(warp::reply::with_status(
                        json,
                        StatusCode::from_u16(e_msg.code)
                            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                    ))
                } else {
                    //other internal error - will return 500 internal server error
                    Err(r)
                }
            });

            info!(message = "Building HTTP server.", address = %address);

            let listener = tls.bind(&address).await.unwrap();
            let _ = warp::serve(routes)
                .serve_incoming_with_graceful_shutdown(
                    listener.accept_stream(),
                    shutdown.clone().map(|_| ()),
                )
                .await;
            // We need to drop the last copy of ShutdownSignalToken only after server has shut down.
            drop(shutdown);
            Ok(())
        }))
    }
}
