use crate::{
    runtime::Runtime,
    test_util::runtime,
    topology::config::{SinkConfig, SinkContext},
    Error,
};
use bytes05::Bytes;
use futures::{compat::Future01CompatExt, FutureExt, TryFutureExt};
use futures01::{sync::mpsc, Future, Sink};
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use serde::Deserialize;

pub fn load_sink<T>(config: &str) -> crate::Result<(T, SinkContext, Runtime)>
where
    for<'a> T: Deserialize<'a> + SinkConfig,
{
    let sink_config: T = toml::from_str(config)?;
    let rt = runtime();
    let cx = SinkContext::new_test();

    Ok((sink_config, cx, rt))
}

pub fn build_test_server(
    addr: std::net::SocketAddr,
    rt: &mut Runtime,
) -> (
    mpsc::Receiver<(http::request::Parts, Bytes)>,
    stream_cancel::Trigger,
    impl Future<Item = (), Error = ()>,
) {
    let (tx, rx) = mpsc::channel(100);
    let service = make_service_fn(move |_| {
        let tx = tx.clone();
        async {
            Ok::<_, Error>(service_fn(move |req: Request<Body>| {
                let tx = tx.clone();
                async {
                    let (parts, body) = req.into_parts();
                    tokio::spawn(async move {
                        let bytes = hyper::body::to_bytes(body).await.unwrap();
                        tx.send((parts, bytes)).compat().await.unwrap();
                    });

                    Ok::<_, Error>(Response::new(Body::empty()))
                }
            }))
        }
    });

    let (trigger, tripwire) = stream_cancel::Tripwire::new();
    let server = rt.block_on_std(async move {
        Server::bind(&addr)
            .serve(service)
            .with_graceful_shutdown(tripwire.clone().compat().map(|_| ()))
            .compat()
            .map_err(|e| panic!("server error: {}", e))
    });

    (rx, trigger, server)
}
