use std::{
    convert::Infallible,
    future::{Future, Ready, ready},
    net::SocketAddr,
};

use bytes::Bytes;
use http::{HeaderMap, Method, Request, Response, StatusCode, Uri, header, uri::Scheme};
use http_body_util::{Either, Full};
use hyper_1::{body::Incoming, client::conn, service::service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::{JoinHandle, JoinSet},
    time::timeout,
};

use super::{addr::next_addr, wait_for_tcp};

/// Spawns an HTTP server that uses the given `handler` to respond to requests.
///
/// A random local address is chosen for the HTTP server to listen on, and the function does not return until the server
/// is up and ready for requests. The returned `Uri` is configured for the appropriate address.
///
/// # Panics
///
/// Panics if the listener cannot be bound or its address cannot be used to construct a URI.
pub async fn spawn_blackhole_http_server<H, F>(handler: H) -> Uri
where
    H: Fn(Request<Incoming>) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = std::result::Result<Response<Full<Bytes>>, Infallible>> + Send + 'static,
{
    let (_guard, address) = next_addr();

    let uri = Uri::builder()
        .scheme(Scheme::HTTP)
        .authority(address.to_string())
        .path_and_query("/")
        .build()
        .expect("URI should always be valid when starting from `SocketAddr`");

    let listener = TcpListener::bind(address).await.unwrap();

    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(connection) => connection,
                Err(error) => {
                    error!(message = "Blackhole HTTP server error.", ?error);
                    break;
                }
            };
            let handler = handler.clone();
            tokio::spawn(async move {
                if let Err(error) = auto::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(stream), service_fn(handler))
                    .await
                {
                    error!(message = "Blackhole HTTP server error.", ?error);
                }
            });
        }
    });

    wait_for_tcp(address).await;

    uri
}

/// Responds to every request with a 200 OK response.
pub fn always_200_response(
    _: Request<Incoming>,
) -> Ready<Result<Response<Full<Bytes>>, Infallible>> {
    ready(Ok(Response::new(Full::default())))
}

const PROXY_AUTHORIZATION: &str = "Basic cHJveHktdXNlcjpwcm94eS1wYXNz";

/// A request observed by [`AuthenticatedHttpProxy`] before it was forwarded.
#[derive(Debug)]
pub struct ProxyRequestObservation {
    pub method: Method,
    pub uri: Uri,
    pub headers: HeaderMap,
}

/// A deterministic authenticated plaintext HTTP proxy for component tests.
pub struct AuthenticatedHttpProxy {
    url: String,
    observations: mpsc::UnboundedReceiver<ProxyRequestObservation>,
    task: JoinHandle<()>,
}

impl AuthenticatedHttpProxy {
    /// Returns the proxy URL, including its test credentials.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Waits for the next request received by the proxy.
    ///
    /// # Panics
    ///
    /// Panics if no request arrives within five seconds or the observation channel closes.
    pub async fn next_request(&mut self) -> ProxyRequestObservation {
        timeout(std::time::Duration::from_secs(5), self.observations.recv())
            .await
            .expect("proxy did not receive a request")
            .expect("proxy observation channel closed")
    }
}

impl Drop for AuthenticatedHttpProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Spawns an HTTP proxy requiring the `proxy-user`/`proxy-pass` credentials.
#[must_use]
///
/// # Panics
///
/// Panics if binding or inspecting the listener fails. Spawned tasks panic on invalid HTTP responses.
pub fn spawn_authenticated_http_proxy() -> AuthenticatedHttpProxy {
    let address = SocketAddr::from(([127, 0, 0, 1], 0));
    let listener = std::net::TcpListener::bind(address).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let listener = TcpListener::from_std(listener).unwrap();
    let (observations_tx, observations) = mpsc::unbounded_channel();

    let task = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(connection) => connection,
                Err(error) => {
                    error!(message = "Test HTTP proxy error.", ?error);
                    break;
                }
            };
            while connections.try_join_next().is_some() {}
            let observations_tx = observations_tx.clone();
            connections.spawn(async move {
                let service = service_fn(move |mut request: Request<Incoming>| {
                    let observations_tx = observations_tx.clone();
                    async move {
                        let observation = ProxyRequestObservation {
                            method: request.method().clone(),
                            uri: request.uri().clone(),
                            headers: request.headers().clone(),
                        };
                        let authorized = observation
                            .headers
                            .get(header::PROXY_AUTHORIZATION)
                            .is_some_and(|value| {
                                value.as_bytes() == PROXY_AUTHORIZATION.as_bytes()
                            });
                        _ = observations_tx.send(observation);

                        if !authorized {
                            return Ok::<_, Infallible>(
                                Response::builder()
                                    .status(StatusCode::PROXY_AUTHENTICATION_REQUIRED)
                                    .body(Either::Left(Full::<Bytes>::default()))
                                    .unwrap(),
                            );
                        }

                        request.headers_mut().remove(header::PROXY_AUTHORIZATION);
                        let response = async {
                            if request.uri().scheme() != Some(&Scheme::HTTP) {
                                return Err("proxy requires an absolute HTTP URI".into());
                            }
                            let host = request.uri().host().ok_or("proxy request has no host")?;
                            let port = request.uri().port_u16().unwrap_or(80);
                            let stream =
                                TcpStream::connect((host.trim_matches(['[', ']']), port)).await?;
                            let (mut sender, connection) =
                                conn::http1::handshake(TokioIo::new(stream)).await?;
                            tokio::spawn(async move {
                                if let Err(error) = connection.await {
                                    debug!(
                                        message = "Test HTTP proxy upstream connection closed.",
                                        ?error
                                    );
                                }
                            });
                            if !request.headers().contains_key(header::HOST) {
                                let authority =
                                    request.uri().authority().unwrap().as_str().parse()?;
                                request.headers_mut().insert(header::HOST, authority);
                            }
                            *request.uri_mut() = request
                                .uri()
                                .path_and_query()
                                .map_or("/", |path| path.as_str())
                                .parse()?;
                            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                                sender.send_request(request).await?,
                            )
                        }
                        .await;
                        Ok(match response {
                            Ok(response) => response.map(Either::Right),
                            Err(_) => Response::builder()
                                .status(StatusCode::BAD_GATEWAY)
                                .body(Either::Left(Full::default()))
                                .unwrap(),
                        })
                    }
                });
                if let Err(error) = auto::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(stream), service)
                    .await
                {
                    error!(message = "Test HTTP proxy error.", ?error);
                }
            });
        }
    });

    AuthenticatedHttpProxy {
        url: format!("http://proxy-user:proxy-pass@{address}"),
        observations,
        task,
    }
}
