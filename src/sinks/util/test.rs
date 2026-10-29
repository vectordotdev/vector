use std::{
    io::{BufRead, BufReader},
    net::SocketAddr,
};

use bytes::{Buf, Bytes};
use futures::{FutureExt, SinkExt, channel::mpsc, stream};
use futures_util::StreamExt;
use http::{Request, Response, StatusCode, request::Parts};
use http_body_util::{BodyExt, Full};
use hyper_1::{body::Incoming, service::service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto,
};
use serde::Deserialize;
use stream_cancel::{Trigger, Tripwire};
use vector_common::decompression::CappedDecoder;

use crate::config::{SinkConfig, SinkContext};

/// Load a sink configuration with a default test context.
///
/// # Errors
///
/// Returns an error if the TOML cannot be deserialized as the sink configuration.
pub fn load_sink<T>(config: &str) -> crate::Result<(T, SinkContext)>
where
    for<'a> T: Deserialize<'a> + SinkConfig,
{
    let sink_config: T = toml::from_str(config)?;
    let cx = SinkContext::default();

    Ok((sink_config, cx))
}

/// Load a sink configuration with the supplied test context.
///
/// # Errors
///
/// Returns an error if the TOML cannot be deserialized as the sink configuration.
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
    mpsc::Receiver<(Parts, Bytes)>,
    Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
) {
    build_test_server_generic(addr, || Response::new(Full::new(Bytes::new())))
}

pub fn build_test_server_status(
    addr: SocketAddr,
    status: StatusCode,
) -> (
    mpsc::Receiver<(Parts, Bytes)>,
    Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
) {
    build_test_server_generic(addr, move || {
        Response::builder()
            .status(status)
            .body(Full::new(Bytes::new()))
            .unwrap_or_else(|_| unreachable!())
    })
}

/// Build a test server with a custom response factory.
///
/// # Panics
///
/// Panics if the address cannot be bound or the listener cannot be made nonblocking.
/// The returned future panics on listener errors; spawned request tasks panic if
/// reading or forwarding a body fails.
pub fn build_test_server_generic<B>(
    addr: SocketAddr,
    responder: impl Fn() -> Response<B> + Clone + Send + Sync + 'static,
) -> (
    mpsc::Receiver<(Parts, Bytes)>,
    Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
)
where
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let listener = std::net::TcpListener::bind(addr).expect("Failed to bind test server");
    listener
        .set_nonblocking(true)
        .expect("Failed to make test server listener nonblocking");
    let (tx, rx) = mpsc::channel(100);
    let (trigger, tripwire) = Tripwire::new();
    let server = async move {
        let listener = tokio::net::TcpListener::from_std(listener)
            .expect("Failed to create test server listener");
        let shutdown = tripwire.then(crate::shutdown::tripwire_handler);
        tokio::pin!(shutdown);

        loop {
            let (stream, _) = tokio::select! {
                () = &mut shutdown => return Ok(()),
                accepted = listener.accept() => accepted.expect("Failed to accept test connection"),
            };
            let responder = responder.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let service = service_fn(move |req: Request<Incoming>| {
                    let responder = responder.clone();
                    let mut tx = tx.clone();
                    async move {
                        let (parts, body) = req.into_parts();
                        let bytes = body.collect().await.unwrap().to_bytes();
                        let response = responder();
                        if response.status().is_success() {
                            tokio::spawn(async move {
                                tx.send((parts, bytes)).await.unwrap();
                            });
                        }

                        Ok::<_, std::convert::Infallible>(response)
                    }
                });
                if let Err(error) = auto::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(stream), service)
                    .await
                {
                    tracing::debug!(message = "Test server connection closed.", %error);
                }
            });
        }
    };

    (rx, trigger, server)
}

pub async fn get_received_gzip(
    rx: mpsc::Receiver<(Parts, Bytes)>,
    assert_parts: impl Fn(Parts),
) -> Vec<String> {
    get_received(rx, assert_parts, |body| {
        CappedDecoder::gzip(body.reader()).into_reader()
    })
    .await
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
