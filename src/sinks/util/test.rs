use crate::{
    config::{SinkConfig, SinkContext},
    Error,
};
use bytes::Bytes;
use futures::{channel::mpsc, FutureExt, SinkExt, TryFutureExt};
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use serde::Deserialize;
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
    addr: std::net::SocketAddr,
) -> (
    mpsc::Receiver<(http::request::Parts, Bytes)>,
    Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
) {
    build_test_server_generic(addr, || Response::new(Body::empty()))
}

pub fn build_test_server_generic(
    addr: std::net::SocketAddr,
    responder: impl Fn() -> Response<Body> + Clone + Send + Sync + 'static,
) -> (
    mpsc::Receiver<(http::request::Parts, Bytes)>,
    Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
) {
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
