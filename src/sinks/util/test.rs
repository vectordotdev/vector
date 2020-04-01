use crate::{
    runtime::Runtime,
    topology::config::{SinkConfig, SinkContext},
};
use futures01::{sync::mpsc, Future, Sink, Stream};
use hyper::{service::service_fn_ok, Body, Request, Response, Server};
use serde::Deserialize;

pub fn load_sink<T>(config: &str) -> crate::Result<(T, SinkContext, Runtime)>
where
    for<'a> T: Deserialize<'a> + SinkConfig,
{
    let sink_config: T = toml::from_str(config)?;
    let rt = crate::test_util::runtime();
    let cx = SinkContext::new_test(rt.executor());

    Ok((sink_config, cx, rt))
}

pub fn build_test_server(
    addr: &std::net::SocketAddr,
) -> (
    mpsc::Receiver<(http::request::Parts, hyper::Chunk)>,
    stream_cancel::Trigger,
    impl Future<Item = (), Error = ()>,
) {
    let (tx, rx) = mpsc::channel(100);
    let service = move || {
        let tx = tx.clone();
        service_fn_ok(move |req: Request<Body>| {
            let (parts, body) = req.into_parts();

            let tx = tx.clone();
            tokio01::spawn(
                body.concat2()
                    .map_err(|e| panic!(e))
                    .and_then(|body| tx.send((parts, body)))
                    .map(|_| ())
                    .map_err(|e| panic!(e)),
            );

            Response::new(Body::empty())
        })
    };

    let (trigger, tripwire) = stream_cancel::Tripwire::new();
    let server = Server::bind(addr)
        .serve(service)
        .with_graceful_shutdown(tripwire)
        .map_err(|e| panic!("server error: {}", e));

    (rx, trigger, server)
}
