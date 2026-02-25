use std::{convert::Infallible, future::Future};

use http::{Request, Response, Uri, uri::Scheme};
use hyper::{
    Body, Server,
    service::{make_service_fn, service_fn},
};

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
