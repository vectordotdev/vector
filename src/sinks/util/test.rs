use bytes::{Buf, Bytes};
use flate2::read::{MultiGzDecoder, ZlibDecoder};
use futures::{channel::mpsc, stream, FutureExt, SinkExt, TryFutureExt};
use futures_util::StreamExt;
use http::request::Parts;
use hyper::{
    body::HttpBody,
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server, StatusCode,
};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    net::SocketAddr,
};
use stream_cancel::{Trigger, Tripwire};

use crate::{
    config::{SinkConfig, SinkContext},
    Error,
};

pub fn load_sink<T>(config: &str) -> crate::Result<(T, SinkContext)>
where
    for<'a> T: Deserialize<'a> + SinkConfig,
{
    let sink_config: T = toml::from_str(config)?;
    let cx = SinkContext::default();

    Ok((sink_config, cx))
}

pub fn load_sink_with_context<T>(config: &str, cx: SinkContext) -> crate::Result<(T, SinkContext)>
where
    for<'a> T: Deserialize<'a> + SinkConfig,
{
    let sink_config: T = toml::from_str(config)?;

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
        .with_graceful_shutdown(tripwire.then(crate::shutdown::tripwire_handler))
        .map_err(|error| panic!("Server error: {}", error));

    (rx, trigger, server)
}

pub async fn get_received_gzip(
    rx: mpsc::Receiver<(Parts, Bytes)>,
    assert_parts: impl Fn(Parts),
) -> Vec<String> {
    get_received(rx, assert_parts, |body| MultiGzDecoder::new(body.reader())).await
}

pub async fn get_received_zlib(
    rx: mpsc::Receiver<(Parts, Bytes)>,
    assert_parts: impl Fn(Parts),
) -> Vec<String> {
    get_received(rx, assert_parts, |body| ZlibDecoder::new(body.reader())).await
}

async fn get_received<D>(
    rx: mpsc::Receiver<(Parts, Bytes)>,
    assert_parts: impl Fn(Parts),
    decoder_maker: impl Fn(Bytes) -> D,
) -> Vec<String>
where
    D: std::io::Read,
{
    rx.flat_map(|(parts, body)| {
        assert_parts(parts);
        let decoder = decoder_maker(body);
        let reader = BufReader::new(decoder);
        stream::iter(reader.lines())
    })
    .map(Result::unwrap)
    .map(|line| {
        let val: serde_json::Value = serde_json::from_str(&line).unwrap();
        val.get("message").unwrap().as_str().unwrap().to_owned()
    })
    .collect::<Vec<_>>()
    .await
}
