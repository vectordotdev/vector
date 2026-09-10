use std::{convert::Infallible, future::Future, net::SocketAddr};

use http::{HeaderMap, Method, Request, Response, StatusCode, Uri, header, uri::Scheme};
use hyper::{
    Body, Client, Server,
    service::{make_service_fn, service_fn},
};
use tokio::{sync::mpsc, task::JoinHandle, time::timeout};

use super::{addr::next_addr, wait_for_tcp};

/// Spawns an HTTP server that uses the given `handler` to respond to requests.
///
/// A random local address is chosen for the HTTP server to listen on, and the function does not return until the server
/// is up and ready for requests. The returned `Uri` is configured for the appropriate address.
pub async fn spawn_blackhole_http_server<H, F>(handler: H) -> Uri
where
    H: Fn(Request<Body>) -> F + Clone + Send + 'static,
    F: Future<Output = std::result::Result<Response<Body>, Infallible>> + Send + 'static,
{
    let (_guard, address) = next_addr();

    let uri = Uri::builder()
        .scheme(Scheme::HTTP)
        .authority(address.to_string())
        .path_and_query("/")
        .build()
        .expect("URI should always be valid when starting from `SocketAddr`");

    let make_service = make_service_fn(move |_| {
        let handler = handler.clone();
        let service = service_fn(handler);

        async move { Ok::<_, Infallible>(service) }
    });

    let server = Server::bind(&address).serve(make_service);

    tokio::spawn(async move {
        if let Err(error) = server.await {
            error!(message = "Blackhole HTTP server error.", ?error);
        }
    });

    wait_for_tcp(address).await;

    uri
}

/// Responds to every request with a 200 OK response.
pub async fn always_200_response(_: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::empty()))
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
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Waits for the next request received by the proxy.
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
pub fn spawn_authenticated_http_proxy() -> AuthenticatedHttpProxy {
    let address = SocketAddr::from(([127, 0, 0, 1], 0));
    let server = Server::bind(&address);
    let address = server.local_addr();
    let (observations_tx, observations) = mpsc::unbounded_channel();

    let make_service = make_service_fn(move |_| {
        let observations_tx = observations_tx.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |mut request: Request<Body>| {
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
                        .is_some_and(|value| value.as_bytes() == PROXY_AUTHORIZATION.as_bytes());
                    _ = observations_tx.send(observation);

                    if !authorized {
                        return Ok::<_, Infallible>(
                            Response::builder()
                                .status(StatusCode::PROXY_AUTHENTICATION_REQUIRED)
                                .body(Body::empty())
                                .unwrap(),
                        );
                    }

                    request.headers_mut().remove(header::PROXY_AUTHORIZATION);
                    let response = Client::new().request(request).await.unwrap_or_else(|_| {
                        Response::builder()
                            .status(StatusCode::BAD_GATEWAY)
                            .body(Body::empty())
                            .unwrap()
                    });
                    Ok(response)
                }
            }))
        }
    });

    let task = tokio::spawn(async move {
        if let Err(error) = server.serve(make_service).await {
            error!(message = "Test HTTP proxy error.", ?error);
        }
    });

    AuthenticatedHttpProxy {
        url: format!("http://proxy-user:proxy-pass@{address}"),
        observations,
        task,
    }
}
