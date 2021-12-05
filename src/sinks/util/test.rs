use crate::{
    config::{SinkConfig, SinkContext},
    Error,
};
use bytes::Bytes;
use futures::{channel::mpsc, FutureExt, SinkExt, TryFutureExt};
use hyper::{
    body::HttpBody,
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server, StatusCode,
};
use serde::Deserialize;
use std::net::SocketAddr;
use stream_cancel::{Trigger, Tripwire};

pub fn load_sink<T>(config: &str) -> crate::Result<(T, SinkContext)>
where
    for<'a> T: Deserialize<'a> + SinkConfig,
{
    let sink_config: T = toml::from_str(config)?;
    let cx = SinkContext::new_test();

    Ok((sink_config, cx))
}

pub fn build_test_server(
    addr: SocketAddr,
) -> (
    mpsc::Receiver<(http::request::Parts, Bytes)>,
    Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
) {
    build_test_server_generic(addr, || Response::new(Body::empty()))
}

pub fn build_test_server_status(
    addr: SocketAddr,
    status: StatusCode,
) -> (
    mpsc::Receiver<(http::request::Parts, Bytes)>,
    Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
) {
    build_test_server_generic(addr, move || {
        Response::builder()
            .status(status)
            .body(Body::empty())
            .unwrap_or_else(|_| unreachable!())
    })
}

pub fn build_test_server_generic<B>(
    addr: SocketAddr,
    responder: impl Fn() -> Response<B> + Clone + Send + Sync + 'static,
) -> (
    mpsc::Receiver<(http::request::Parts, Bytes)>,
    Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
)
where
    B: HttpBody + Send + 'static,
    <B as HttpBody>::Data: Send + Sync,
    <B as HttpBody>::Error: snafu::Error + Send + Sync,
{
    let (tx, rx) = mpsc::channel(100);
    let service = make_service_fn(move |_| {
        let responder = responder.clone();
        let tx = tx.clone();
        async move {
            let responder = responder.clone();
            Ok::<_, Error>(service_fn(move |req: Request<Body>| {
                let responder = responder.clone();
                let mut tx = tx.clone();
                async move {
                    let (parts, body) = req.into_parts();
                    let response = responder();
                    if response.status().is_success() {
                        tokio::spawn(async move {
                            let bytes = hyper::body::to_bytes(body).await.unwrap();
                            tx.send((parts, bytes)).await.unwrap();
                        });
                    }

                    Ok::<_, Error>(response)
                }
            }))
        }
    });

    let (trigger, tripwire) = Tripwire::new();
    let server = Server::bind(&addr)
        .serve(service)
        .with_graceful_shutdown(tripwire.then(crate::stream::tripwire_handler))
        .map_err(|error| panic!("Server error: {}", error));

    (rx, trigger, server)
}
