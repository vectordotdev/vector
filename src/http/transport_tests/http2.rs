//! Native HTTP/2 transport coverage for [`Http2Client`].
//!
//! The servers below are Hyper 0.14 HTTP/2 servers using `http` 0.2 types while the client under
//! test uses native `http` 1 types, so every case also proves wire interoperability. The two type
//! families only meet at the socket.

use std::{
    error::Error,
    future::Future,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode, header};
use http_body_util::{BodyExt as _, Empty, StreamBody};
use hyper::{Body, body::HttpBody as _, server::conn::http2, service::service_fn};
use hyper_1::body::{Frame, Incoming};
use hyper_util::{client::legacy::Builder, rt::TokioExecutor};
use rstest::rstest;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _, copy_bidirectional},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::{JoinHandle, JoinSet},
    time::timeout,
};
use tokio_stream::wrappers::ReceiverStream;

use super::{
    INVALID_CA_PATH, REQUEST_TIMEOUT, RequestObservation, TestServer, bad_gateway, client_tls,
    has_proxy_authorization, no_tls, proxy_config, server_tls_with_alpn, spawn_proxy,
};
use crate::{
    config::ProxyConfig,
    http::client_v1::Http2Client,
    tls::{MaybeTlsIncomingStream, MaybeTlsSettings, TEST_PEM_CA_PATH, TlsConfig},
};

type BoxError = Box<dyn Error + Send + Sync>;
type StreamingBody = StreamBody<ReceiverStream<Result<Frame<Bytes>, BoxError>>>;

const STREAM_PATH: &str = "/stream";
const GRPC_PATH: &str = "/grpc";
const GRPC_MESSAGE: &[u8] = b"grpc payload";
const DESTINATION_TOKEN: &str = "Bearer destination-token";
const H2_ALPN: &[u8] = b"h2";
const HTTP1_ALPN: &[u8] = b"http/1.1";
const KEEPALIVE_INTERVAL: Duration = Duration::from_millis(100);
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(1);
const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
const H2_FRAME_HEADER_LEN: usize = 9;
const H2_PING_FRAME: u8 = 0x6;
const H2_ACK_FLAG: u8 = 0x1;

/// Spawns futures created by the legacy Hyper 0.14 HTTP/2 server and client connections.
#[derive(Clone, Copy)]
struct TokioSpawn;

impl<F> hyper::rt::Executor<F> for TokioSpawn
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, future: F) {
        tokio::spawn(future);
    }
}

/// A test server that also reports the ALPN negotiated on each accepted connection.
struct AlpnServer {
    server: TestServer,
    alpn: mpsc::UnboundedReceiver<Option<Vec<u8>>>,
}

impl AlpnServer {
    async fn next_alpn(&mut self) -> Option<Vec<u8>> {
        timeout(REQUEST_TIMEOUT, self.alpn.recv())
            .await
            .expect("the server must accept a connection")
            .expect("the server stopped")
    }
}

/// Spawns a legacy HTTP/2-only server that records every request before handing it to `handler`.
async fn spawn_h2_server<H, F>(tls: MaybeTlsSettings, handler: H) -> AlpnServer
where
    H: Fn(Request<Body>) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = Result<Response<Body>, hyper::Error>> + Send + 'static,
{
    let addr = "127.0.0.1:0".parse().unwrap();
    let mut listener = tls.bind(&addr).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let observations = Arc::new(Mutex::new(Vec::new()));
    let server_observations = Arc::clone(&observations);
    let (alpn_tx, alpn) = mpsc::unbounded_channel();

    let task = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            let Ok(mut stream) = listener.accept().await else {
                continue;
            };
            while connections.try_join_next().is_some() {}
            let observations = Arc::clone(&server_observations);
            let handler = handler.clone();
            let alpn_tx = alpn_tx.clone();
            connections.spawn(async move {
                if stream.handshake().await.is_err() {
                    return;
                }
                _ = alpn_tx.send(negotiated_alpn(&stream));
                let service = service_fn(move |request: Request<Body>| {
                    observations.lock().unwrap().push(RequestObservation {
                        method: request.method().clone(),
                        uri: request.uri().clone(),
                        headers: request.headers().clone(),
                    });
                    handler(request)
                });
                if let Err(error) = http2::Builder::new(TokioSpawn)
                    .serve_connection(stream, service)
                    .await
                {
                    tracing::debug!(message = "HTTP/2 server connection closed.", %error);
                }
            });
        }
    });

    AlpnServer {
        server: TestServer {
            addr,
            observations,
            task,
        },
        alpn,
    }
}

fn negotiated_alpn(stream: &MaybeTlsIncomingStream<TcpStream>) -> Option<Vec<u8>> {
    stream
        .ssl_stream()
        .and_then(|stream| stream.ssl().selected_alpn_protocol())
        .map(<[u8]>::to_vec)
}

/// An HTTP/2 origin that reports each streamed request chunk as soon as it arrives.
struct H2Origin {
    h2: AlpnServer,
    chunks: mpsc::UnboundedReceiver<Bytes>,
}

impl H2Origin {
    async fn expect_chunks(&mut self, expected: &[u8]) {
        let mut received = Vec::new();
        while received.len() < expected.len() {
            let chunk = timeout(REQUEST_TIMEOUT, self.chunks.recv())
                .await
                .expect("the origin must observe the request chunk before the body ends")
                .expect("the origin stopped");
            received.extend_from_slice(&chunk);
        }
        assert_eq!(received, expected);
    }
}

async fn spawn_h2_origin(tls: MaybeTlsSettings) -> H2Origin {
    let (chunks_tx, chunks) = mpsc::unbounded_channel();
    let h2 = spawn_h2_server(tls, move |request| {
        origin_response(request, chunks_tx.clone())
    })
    .await;
    H2Origin { h2, chunks }
}

async fn origin_response(
    request: Request<Body>,
    chunks: mpsc::UnboundedSender<Bytes>,
) -> Result<Response<Body>, hyper::Error> {
    match request.uri().path() {
        STREAM_PATH => {
            let mut body = request.into_body();
            let mut received = Vec::new();
            while let Some(chunk) = body.data().await {
                let chunk = chunk?;
                received.extend_from_slice(&chunk);
                _ = chunks.send(chunk);
            }
            Ok(Response::new(Body::from(received)))
        }
        GRPC_PATH => Ok(grpc_response()),
        _ => Ok(Response::new(Body::from("origin response"))),
    }
}

fn grpc_response() -> Response<Body> {
    let (mut sender, body) = Body::channel();
    tokio::spawn(async move {
        if sender
            .send_data(Bytes::from_static(GRPC_MESSAGE))
            .await
            .is_ok()
        {
            let mut trailers = HeaderMap::new();
            trailers.insert("grpc-status", HeaderValue::from_static("5"));
            trailers.insert("grpc-message", HeaderValue::from_static("missing"));
            _ = sender.send_trailers(trailers).await;
        }
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "application/grpc")
        .body(body)
        .unwrap()
}

/// An HTTP/2 forward proxy: it requires the test proxy credentials and forwards absolute-form
/// h2c requests to their `:authority`.
async fn spawn_h2_forward_proxy(tls: MaybeTlsSettings) -> TestServer {
    spawn_h2_server(tls, forward_h2).await.server
}

async fn forward_h2(mut request: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    if !has_proxy_authorization(request.headers()) {
        return Ok(Response::builder()
            .status(StatusCode::PROXY_AUTHENTICATION_REQUIRED)
            .body(Body::empty())
            .unwrap());
    }
    let Some(authority) = request.uri().authority().cloned() else {
        return Ok(bad_gateway());
    };
    let Ok(stream) = TcpStream::connect(authority.as_str()).await else {
        return Ok(bad_gateway());
    };
    let (mut sender, connection) =
        hyper::client::conn::http2::handshake(TokioSpawn, stream).await?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            tracing::debug!(message = "Forward proxy upstream connection closed.", %error);
        }
    });
    request.headers_mut().remove(header::PROXY_AUTHORIZATION);
    sender.send_request(request).await
}

/// Terminates TLS in front of a plaintext proxy, reporting the ALPN negotiated by each client.
async fn spawn_tls_front(upstream: SocketAddr) -> AlpnServer {
    let tls = server_tls_with_alpn(false, Some(vec!["h2".to_owned(), "http/1.1".to_owned()]));
    let addr = "127.0.0.1:0".parse().unwrap();
    let mut listener = tls.bind(&addr).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (alpn_tx, alpn) = mpsc::unbounded_channel();

    let task = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            let Ok(mut stream) = listener.accept().await else {
                continue;
            };
            while connections.try_join_next().is_some() {}
            let alpn_tx = alpn_tx.clone();
            connections.spawn(async move {
                if stream.handshake().await.is_err() {
                    return;
                }
                _ = alpn_tx.send(negotiated_alpn(&stream));
                let Ok(mut upstream) = TcpStream::connect(upstream).await else {
                    return;
                };
                if let Err(error) = copy_bidirectional(&mut stream, &mut upstream).await {
                    tracing::debug!(message = "TLS proxy front closed.", %error);
                }
            });
        }
    });

    AlpnServer {
        server: TestServer {
            addr,
            observations: Arc::new(Mutex::new(Vec::new())),
            task,
        },
        alpn,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RelayEvent {
    /// The client sent a PING frame after the test marked the connection idle.
    IdlePing,
    /// The client closed its side of the connection.
    ClientClosed,
}

/// A TCP relay in front of an h2c origin that inspects client HTTP/2 frames on the wire.
///
/// When `withhold_ack` is set, the first idle PING switches its connection to discard everything
/// the origin sends back, so the PING acknowledgement never reaches the client.
struct PingRelay {
    addr: SocketAddr,
    idle: Arc<AtomicBool>,
    connections: Arc<AtomicUsize>,
    events: mpsc::UnboundedReceiver<(usize, RelayEvent)>,
    task: JoinHandle<()>,
}

impl PingRelay {
    fn http_uri(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn mark_idle(&self) {
        self.idle.store(true, Ordering::SeqCst);
    }

    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    async fn next_event(&mut self) -> (usize, RelayEvent) {
        timeout(REQUEST_TIMEOUT, self.events.recv())
            .await
            .expect("relay event timed out")
            .expect("the relay stopped")
    }
}

impl Drop for PingRelay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn spawn_ping_relay(upstream: SocketAddr, withhold_ack: bool) -> PingRelay {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let idle = Arc::new(AtomicBool::new(false));
    let withhold_ack = Arc::new(AtomicBool::new(withhold_ack));
    let connections = Arc::new(AtomicUsize::new(0));
    let (events_tx, events) = mpsc::unbounded_channel();

    let relay_idle = Arc::clone(&idle);
    let relay_connections = Arc::clone(&connections);
    let task = tokio::spawn(async move {
        let mut tasks = JoinSet::new();
        loop {
            let Ok((client, _)) = listener.accept().await else {
                continue;
            };
            while tasks.try_join_next().is_some() {}
            let index = relay_connections.fetch_add(1, Ordering::SeqCst);
            tasks.spawn(relay_connection(
                index,
                client,
                upstream,
                Arc::clone(&relay_idle),
                Arc::clone(&withhold_ack),
                events_tx.clone(),
            ));
        }
    });

    PingRelay {
        addr,
        idle,
        connections,
        events,
        task,
    }
}

async fn relay_connection(
    index: usize,
    client: TcpStream,
    upstream: SocketAddr,
    idle: Arc<AtomicBool>,
    withhold_ack: Arc<AtomicBool>,
    events: mpsc::UnboundedSender<(usize, RelayEvent)>,
) {
    let Ok(server) = TcpStream::connect(upstream).await else {
        return;
    };
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();
    let discard_downstream = Arc::new(AtomicBool::new(false));

    let upstream_discard = Arc::clone(&discard_downstream);
    let to_server = async move {
        let mut scanner = FrameScanner::default();
        let mut buffer = vec![0; 16 * 1024];
        loop {
            let read = match client_read.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            };
            if scanner.client_pings(&buffer[..read]) > 0 && idle.load(Ordering::SeqCst) {
                // Decide before forwarding the PING so its acknowledgement cannot race this flag.
                if withhold_ack.swap(false, Ordering::SeqCst) {
                    upstream_discard.store(true, Ordering::SeqCst);
                }
                _ = events.send((index, RelayEvent::IdlePing));
            }
            if server_write.write_all(&buffer[..read]).await.is_err() {
                break;
            }
        }
        _ = events.send((index, RelayEvent::ClientClosed));
    };
    let to_client = async move {
        let mut buffer = vec![0; 16 * 1024];
        loop {
            let read = match server_read.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            };
            if discard_downstream.load(Ordering::SeqCst) {
                continue;
            }
            if client_write.write_all(&buffer[..read]).await.is_err() {
                break;
            }
        }
    };
    tokio::join!(to_server, to_client);
}

/// Incrementally splits the client side of an HTTP/2 connection into frames.
#[derive(Default)]
struct FrameScanner {
    pending: Vec<u8>,
    preface_seen: bool,
}

impl FrameScanner {
    /// Returns the number of non-acknowledgement PING frames completed by `bytes`.
    fn client_pings(&mut self, bytes: &[u8]) -> usize {
        self.pending.extend_from_slice(bytes);
        let mut offset = 0;
        if !self.preface_seen {
            if self.pending.len() < H2_PREFACE.len() {
                return 0;
            }
            assert_eq!(&self.pending[..H2_PREFACE.len()], H2_PREFACE);
            offset = H2_PREFACE.len();
            self.preface_seen = true;
        }

        let mut pings = 0;
        while let Some(frame_header) = self.pending.get(offset..offset + H2_FRAME_HEADER_LEN) {
            let length = (usize::from(frame_header[0]) << 16)
                | (usize::from(frame_header[1]) << 8)
                | usize::from(frame_header[2]);
            let end = offset + H2_FRAME_HEADER_LEN + length;
            if self.pending.len() < end {
                break;
            }
            if frame_header[3] == H2_PING_FRAME && frame_header[4] & H2_ACK_FLAG == 0 {
                pings += 1;
            }
            offset = end;
        }
        self.pending.drain(..offset);
        pings
    }
}

#[derive(Debug)]
struct H2Response {
    status: http_1::StatusCode,
    body: Bytes,
    trailers: Option<http_1::HeaderMap>,
}

fn h2_server_tls() -> MaybeTlsSettings {
    server_tls_with_alpn(false, Some(vec!["h2".to_owned()]))
}

fn h2_client_tls(ca_file: &str) -> MaybeTlsSettings {
    client_tls(TlsConfig {
        ca_file: Some(ca_file.into()),
        alpn_protocols: Some(vec!["h2".to_owned()]),
        ..Default::default()
    })
}

fn client(tls: MaybeTlsSettings, proxy: &ProxyConfig) -> Http2Client<Empty<Bytes>> {
    Http2Client::new(tls, proxy, Builder::new(TokioExecutor::new())).unwrap()
}

fn keepalive_client(keepalive_timeout: Duration) -> Http2Client<Empty<Bytes>> {
    let mut builder = Builder::new(TokioExecutor::new());
    builder
        .http2_keep_alive_interval(KEEPALIVE_INTERVAL)
        .http2_keep_alive_timeout(keepalive_timeout)
        .http2_keep_alive_while_idle(true);
    Http2Client::new(no_tls(), &ProxyConfig::default(), builder).unwrap()
}

async fn get(
    client: &Http2Client<Empty<Bytes>>,
    uri: &str,
    headers: &[(http_1::header::HeaderName, &'static str)],
) -> Result<H2Response, String> {
    let mut request = http_1::Request::get(uri)
        .body(Empty::new())
        .map_err(|error| error.to_string())?;
    for (name, value) in headers {
        request
            .headers_mut()
            .insert(name.clone(), http_1::HeaderValue::from_static(value));
    }
    let response = timeout(REQUEST_TIMEOUT, client.request(request))
        .await
        .map_err(|_| "request timed out".to_owned())?
        .map_err(|error| error.to_string())?;
    collect(response).await
}

async fn collect(response: http_1::Response<Incoming>) -> Result<H2Response, String> {
    let status = response.status();
    let collected = timeout(REQUEST_TIMEOUT, response.into_body().collect())
        .await
        .map_err(|_| "response body timed out".to_owned())?
        .map_err(|error| error.to_string())?;
    let trailers = collected.trailers().cloned();
    Ok(H2Response {
        status,
        body: collected.to_bytes(),
        trailers,
    })
}

fn assert_origin_response(response: &H2Response) {
    assert_eq!(response.status, http_1::StatusCode::OK);
    assert_eq!(response.body.as_ref(), b"origin response");
}

/// Asserts that proxy credentials stayed on the CONNECT request and destination credentials
/// stayed inside the tunnel.
fn assert_connect_credentials_isolated(proxy: &TestServer, origin: &TestServer) {
    let proxy_observations = proxy.observations();
    assert_eq!(proxy_observations.len(), 1);
    assert_eq!(proxy_observations[0].method, Method::CONNECT);
    assert_eq!(
        proxy_observations[0].uri.authority().unwrap().as_str(),
        format!("localhost:{}", origin.addr.port())
    );
    assert!(has_proxy_authorization(&proxy_observations[0].headers));
    assert!(
        !proxy_observations[0]
            .headers
            .contains_key(header::AUTHORIZATION)
    );

    let origin_observations = origin.observations();
    assert_eq!(origin_observations.len(), 1);
    let origin_headers = &origin_observations[0].headers;
    assert!(!origin_headers.contains_key(header::PROXY_AUTHORIZATION));
    assert_eq!(
        origin_headers.get(header::AUTHORIZATION).unwrap(),
        DESTINATION_TOKEN
    );
}

#[tokio::test]
async fn h2c_uses_prior_knowledge_with_destination_authority() {
    let origin = spawn_h2_origin(no_tls()).await;
    let client = client(no_tls(), &ProxyConfig::default());

    assert_origin_response(
        &get(&client, &origin.h2.server.http_uri(), &[])
            .await
            .unwrap(),
    );

    let observations = origin.h2.server.observations();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].method, Method::GET);
    assert_eq!(
        observations[0].uri.to_string(),
        format!("{}/", origin.h2.server.http_uri())
    );
}

#[tokio::test]
async fn h2_over_trusted_tls_offers_configured_alpn() {
    let mut origin = spawn_h2_origin(h2_server_tls()).await;
    let client = client(h2_client_tls(TEST_PEM_CA_PATH), &ProxyConfig::default());

    assert_origin_response(
        &get(&client, &origin.h2.server.https_uri(), &[])
            .await
            .unwrap(),
    );
    assert_eq!(origin.h2.next_alpn().await.as_deref(), Some(H2_ALPN));
}

#[tokio::test]
async fn h2_over_tls_rejects_untrusted_ca() {
    let origin = spawn_h2_origin(h2_server_tls()).await;
    let client = client(h2_client_tls(INVALID_CA_PATH), &ProxyConfig::default());

    get(&client, &origin.h2.server.https_uri(), &[])
        .await
        .expect_err("an untrusted server certificate must fail");
    assert!(origin.h2.server.observations().is_empty());
}

#[rstest]
#[case::h2c(false)]
#[case::tls(true)]
#[tokio::test]
async fn streaming_request_body_is_sent_before_it_completes(#[case] tls: bool) {
    let mut origin = spawn_h2_origin(if tls { h2_server_tls() } else { no_tls() }).await;
    let tls_settings = if tls {
        h2_client_tls(TEST_PEM_CA_PATH)
    } else {
        no_tls()
    };
    let client: Http2Client<StreamingBody> = Http2Client::new(
        tls_settings,
        &ProxyConfig::default(),
        Builder::new(TokioExecutor::new()),
    )
    .unwrap();
    let base = if tls {
        origin.h2.server.https_uri()
    } else {
        origin.h2.server.http_uri()
    };

    let (body_tx, body_rx) = mpsc::channel(1);
    let request = http_1::Request::post(format!("{base}{STREAM_PATH}"))
        .body(StreamBody::new(ReceiverStream::new(body_rx)))
        .unwrap();
    let response = tokio::spawn(client.request(request));

    body_tx
        .send(Ok(Frame::data(Bytes::from_static(b"first "))))
        .await
        .unwrap();
    origin.expect_chunks(b"first ").await;
    body_tx
        .send(Ok(Frame::data(Bytes::from_static(b"second"))))
        .await
        .unwrap();
    drop(body_tx);

    let response = timeout(REQUEST_TIMEOUT, response)
        .await
        .expect("request timed out")
        .unwrap()
        .unwrap();
    let response = collect(response).await.unwrap();
    assert_eq!(response.status, http_1::StatusCode::OK);
    assert_eq!(response.body.as_ref(), b"first second");
}

#[tokio::test]
async fn grpc_trailers_reach_the_caller() {
    let origin = spawn_h2_origin(no_tls()).await;
    let client = client(no_tls(), &ProxyConfig::default());

    let response = get(
        &client,
        &format!("{}{GRPC_PATH}", origin.h2.server.http_uri()),
        &[],
    )
    .await
    .unwrap();

    assert_eq!(response.status, http_1::StatusCode::OK);
    assert_eq!(response.body.as_ref(), GRPC_MESSAGE);
    let trailers = response
        .trailers
        .expect("the gRPC status trailers must arrive");
    assert_eq!(trailers["grpc-status"], "5");
    assert_eq!(trailers["grpc-message"], "missing");
}

#[tokio::test]
async fn h2_via_authenticated_http_connect_proxy() {
    let mut origin = spawn_h2_origin(h2_server_tls()).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let client = client(
        h2_client_tls(TEST_PEM_CA_PATH),
        &proxy_config(&proxy, false, true),
    );

    assert_origin_response(
        &get(
            &client,
            &origin.h2.server.https_uri(),
            &[(http_1::header::AUTHORIZATION, DESTINATION_TOKEN)],
        )
        .await
        .unwrap(),
    );
    assert_connect_credentials_isolated(&proxy, &origin.h2.server);
    assert_eq!(origin.h2.next_alpn().await.as_deref(), Some(H2_ALPN));
}

#[tokio::test]
async fn h2_via_authenticated_tls_connect_proxy_keeps_proxy_on_http1() {
    let mut origin = spawn_h2_origin(h2_server_tls()).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let mut proxy_front = spawn_tls_front(proxy.addr).await;
    let client = client(
        h2_client_tls(TEST_PEM_CA_PATH),
        &proxy_config(&proxy_front.server, true, true),
    );

    assert_origin_response(
        &get(
            &client,
            &origin.h2.server.https_uri(),
            &[(http_1::header::AUTHORIZATION, DESTINATION_TOKEN)],
        )
        .await
        .unwrap(),
    );
    assert_eq!(proxy_front.next_alpn().await.as_deref(), Some(HTTP1_ALPN));
    assert_eq!(origin.h2.next_alpn().await.as_deref(), Some(H2_ALPN));
    assert_connect_credentials_isolated(&proxy, &origin.h2.server);
}

#[rstest]
#[case::plaintext_proxy(false)]
#[case::tls_proxy(true)]
#[tokio::test]
async fn h2c_via_authenticated_forward_proxy(#[case] proxy_tls: bool) {
    let origin = spawn_h2_origin(no_tls()).await;
    let (proxy_server_tls, client_tls_settings) = if proxy_tls {
        (h2_server_tls(), h2_client_tls(TEST_PEM_CA_PATH))
    } else {
        (no_tls(), no_tls())
    };
    let proxy = spawn_h2_forward_proxy(proxy_server_tls).await;
    let client = client(client_tls_settings, &proxy_config(&proxy, proxy_tls, true));

    assert_origin_response(
        &get(
            &client,
            &origin.h2.server.http_uri(),
            &[(http_1::header::AUTHORIZATION, DESTINATION_TOKEN)],
        )
        .await
        .unwrap(),
    );

    let proxy_observations = proxy.observations();
    assert_eq!(proxy_observations.len(), 1);
    assert_eq!(proxy_observations[0].method, Method::GET);
    assert_eq!(
        proxy_observations[0].uri.to_string(),
        format!("{}/", origin.h2.server.http_uri())
    );
    assert!(has_proxy_authorization(&proxy_observations[0].headers));

    let origin_observations = origin.h2.server.observations();
    assert_eq!(origin_observations.len(), 1);
    assert_eq!(
        origin_observations[0]
            .headers
            .get(header::AUTHORIZATION)
            .unwrap(),
        DESTINATION_TOKEN
    );
}

#[tokio::test]
async fn no_proxy_bypass_sends_no_proxy_credentials() {
    let origin = spawn_h2_origin(no_tls()).await;
    let proxy = spawn_h2_forward_proxy(no_tls()).await;
    let mut config = proxy_config(&proxy, false, true);
    config.no_proxy = "127.0.0.1".into();
    let client = client(no_tls(), &config);

    assert_origin_response(
        &get(&client, &origin.h2.server.http_ip_uri(), &[])
            .await
            .unwrap(),
    );

    assert!(proxy.observations().is_empty());
    let origin_observations = origin.h2.server.observations();
    assert_eq!(origin_observations.len(), 1);
    assert!(
        !origin_observations[0]
            .headers
            .contains_key(header::PROXY_AUTHORIZATION)
    );
}

#[tokio::test]
async fn keepalive_pings_idle_connection_and_keeps_it_reusable() {
    let origin = spawn_h2_origin(no_tls()).await;
    let mut relay = spawn_ping_relay(origin.h2.server.addr, false).await;
    let client = keepalive_client(REQUEST_TIMEOUT);

    assert_origin_response(&get(&client, &relay.http_uri(), &[]).await.unwrap());
    relay.mark_idle();
    assert_eq!(relay.next_event().await, (0, RelayEvent::IdlePing));

    assert_origin_response(&get(&client.clone(), &relay.http_uri(), &[]).await.unwrap());
    assert_eq!(relay.connections(), 1);
}

#[tokio::test]
async fn unacknowledged_keepalive_evicts_idle_connection() {
    let origin = spawn_h2_origin(no_tls()).await;
    let mut relay = spawn_ping_relay(origin.h2.server.addr, true).await;
    let client = keepalive_client(KEEPALIVE_TIMEOUT);

    assert_origin_response(&get(&client, &relay.http_uri(), &[]).await.unwrap());
    relay.mark_idle();
    assert_eq!(relay.next_event().await, (0, RelayEvent::IdlePing));
    assert_eq!(relay.next_event().await, (0, RelayEvent::ClientClosed));

    assert_origin_response(&get(&client.clone(), &relay.http_uri(), &[]).await.unwrap());
    assert_eq!(relay.connections(), 2);
}
