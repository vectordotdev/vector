//! Coverage for streaming request bodies on the native transports, the AWS SDK transport, and
//! the HTTP client of the AWS service clients: data frames sent before the body ends, trailers,
//! size hints and the end of the stream, body errors, cancellation, and backpressure.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use futures::stream;
use http::{HeaderMap, HeaderValue, Request, StatusCode, Version, header};
use http_body::{Body, Frame};
use http_body_util::{BodyExt, Empty, Full, StreamBody};
use hyper::body::Incoming;
use hyper_util::{client::legacy::Builder, rt::TokioExecutor};
use rstest::rstest;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::timeout,
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use vector_http::{Http2Client, RequestBody};

use super::{
    HttpClient, REQUEST_TIMEOUT, TestServer, client_tls, no_tls, server_tls_with_alpn, spawn_server,
};
#[cfg(feature = "aws-core")]
use super::{HttpConnectorSettings, HttpRequest, SdkBody, SdkHttpClient, test_runtime_components};
use crate::{
    Error,
    config::ProxyConfig,
    tls::{TEST_PEM_CA_PATH, TlsConfig},
};
#[cfg(feature = "aws-core")]
use aws_config::Region;
#[cfg(feature = "aws-core")]
use aws_smithy_runtime_api::client::http::{HttpClient as _, HttpConnector as _};

type BodyFrame = Result<Frame<Bytes>, Error>;
type ChannelBody = StreamBody<UnboundedReceiverStream<BodyFrame>>;

const CHECKSUM_TRAILER: &str = "x-checksum";

/// A request as the test server received it. The server answers `200 OK` once `respond` is
/// dropped, so the test controls when the response is sent.
struct Received {
    request: Request<Incoming>,
    respond: oneshot::Sender<()>,
}

/// Spawns a server that hands every request to the test, without reading its body.
async fn spawn_receiving_server(
    tls: crate::tls::MaybeTlsSettings,
) -> (TestServer, mpsc::UnboundedReceiver<Received>) {
    let (received_tx, received_rx) = mpsc::unbounded_channel();
    let server = spawn_server(tls, move |request| {
        let received_tx = received_tx.clone();
        async move {
            let (respond, responded) = oneshot::channel();
            if received_tx.send(Received { request, respond }).is_ok() {
                // The test drops `respond` when the server may answer.
                let _responded = responded.await;
            }
            http::Response::new(Full::default())
        }
    })
    .await;
    (server, received_rx)
}

#[derive(Clone, Copy, Debug)]
enum Transport {
    /// HTTP/1.1 through [`HttpClient`].
    Http1,
    /// HTTP/2 negotiated with ALPN through [`HttpClient`].
    H2Alpn,
    /// HTTP/2 with prior knowledge through [`Http2Client`].
    H2c,
    /// HTTP/1.1 through the AWS SDK transport alone.
    #[cfg(feature = "aws-core")]
    Aws,
    /// HTTP/1.1 through the HTTP client of the AWS service clients, which wraps the AWS SDK
    /// transport and counts the bytes sent.
    #[cfg(feature = "aws-core")]
    AwsService,
    /// HTTP/2 negotiated with ALPN through the HTTP client of the AWS service clients.
    #[cfg(feature = "aws-core")]
    AwsServiceH2Alpn,
}

impl Transport {
    const fn version(self) -> Version {
        match self {
            Self::H2Alpn | Self::H2c => Version::HTTP_2,
            Self::Http1 => Version::HTTP_11,
            #[cfg(feature = "aws-core")]
            Self::Aws | Self::AwsService => Version::HTTP_11,
            #[cfg(feature = "aws-core")]
            Self::AwsServiceH2Alpn => Version::HTTP_2,
        }
    }

    const fn negotiates_h2(self) -> bool {
        match self {
            Self::H2Alpn => true,
            #[cfg(feature = "aws-core")]
            Self::AwsServiceH2Alpn => true,
            _ => false,
        }
    }

    async fn spawn_server(self) -> (TestServer, mpsc::UnboundedReceiver<Received>) {
        let tls = if self.negotiates_h2() {
            server_tls_with_alpn(false, Some(vec!["h2".to_owned()]))
        } else {
            no_tls()
        };
        spawn_receiving_server(tls).await
    }

    fn uri(self, server: &TestServer) -> String {
        if self.negotiates_h2() {
            server.https_uri()
        } else {
            server.http_uri()
        }
    }

    /// Sends `request` with a new client in a task, so the test can drive the body and the
    /// server meanwhile.
    fn spawn<B>(self, request: Request<B>) -> JoinHandle<Result<StatusCode, String>>
    where
        B: Body<Data = Bytes, Error = Error> + Send + Sync + 'static,
    {
        let proxy = ProxyConfig::default();
        match self {
            Self::Http1 | Self::H2Alpn => {
                let tls = match self {
                    Self::H2Alpn => client_tls(h2_alpn_tls()),
                    _ => no_tls(),
                };
                let client = HttpClient::new(tls, &proxy).unwrap();
                let request = request.map(BodyExt::boxed_unsync);
                tokio::spawn(async move {
                    client
                        .send(request)
                        .await
                        .map(|response| response.status())
                        .map_err(|error| error.to_string())
                })
            }
            Self::H2c => {
                let client: Http2Client<RequestBody> =
                    Http2Client::new(no_tls(), &proxy, Builder::new(TokioExecutor::new())).unwrap();
                let response = client.request(request.map(BodyExt::boxed_unsync));
                tokio::spawn(async move {
                    response
                        .await
                        .map(|response| response.status())
                        .map_err(|error| error.to_string())
                })
            }
            #[cfg(feature = "aws-core")]
            Self::Aws | Self::AwsService | Self::AwsServiceH2Alpn => {
                let settings = HttpConnectorSettings::default();
                let components = test_runtime_components();
                let connector = if matches!(self, Self::Aws) {
                    SdkHttpClient::new(no_tls(), &proxy)
                        .unwrap()
                        .http_connector(&settings, &components)
                } else {
                    let tls = self.negotiates_h2().then(h2_alpn_tls);
                    crate::aws::service_http_client(&proxy, tls.as_ref(), Region::new("us-east-1"))
                        .unwrap()
                        .http_connector(&settings, &components)
                };
                let request = HttpRequest::try_from(request.map(SdkBody::from_body_1_x)).unwrap();
                tokio::spawn(async move {
                    connector
                        .call(request)
                        .await
                        .map(|response| StatusCode::from_u16(response.status().as_u16()).unwrap())
                        .map_err(|error| error.to_string())
                })
            }
        }
    }
}

/// Client TLS settings that trust the test CA and offer only HTTP/2.
fn h2_alpn_tls() -> TlsConfig {
    TlsConfig {
        ca_file: Some(TEST_PEM_CA_PATH.into()),
        alpn_protocols: Some(vec!["h2".to_owned()]),
        ..Default::default()
    }
}

fn channel_body() -> (mpsc::UnboundedSender<BodyFrame>, ChannelBody) {
    let (frames, receiver) = mpsc::unbounded_channel();
    (
        frames,
        StreamBody::new(UnboundedReceiverStream::new(receiver)),
    )
}

fn data(bytes: &'static [u8]) -> Frame<Bytes> {
    Frame::data(Bytes::from_static(bytes))
}

fn full(bytes: &'static [u8]) -> impl Body<Data = Bytes, Error = Error> + Send + Sync + 'static {
    Full::new(Bytes::from_static(bytes)).map_err(|never| match never {})
}

fn empty() -> impl Body<Data = Bytes, Error = Error> + Send + Sync + 'static {
    Empty::<Bytes>::new().map_err(|never| match never {})
}

async fn next_request(received: &mut mpsc::UnboundedReceiver<Received>) -> Received {
    timeout(REQUEST_TIMEOUT, received.recv())
        .await
        .expect("the server receives the request")
        .expect("the server is running")
}

async fn next_frame(body: &mut Incoming) -> Option<Result<Frame<Bytes>, hyper::Error>> {
    timeout(REQUEST_TIMEOUT, body.frame())
        .await
        .expect("the request body makes progress")
}

async fn next_data(body: &mut Incoming) -> Bytes {
    next_frame(body)
        .await
        .expect("the body has a frame")
        .expect("the frame is valid")
        .into_data()
        .expect("the frame is data")
}

async fn status(response: JoinHandle<Result<StatusCode, String>>) -> Result<StatusCode, String> {
    timeout(REQUEST_TIMEOUT, response)
        .await
        .expect("the request completes")
        .expect("the request task does not panic")
}

/// Each data frame reaches the server as the body yields it, before the body ends. A body of
/// unknown length has no `Content-Length`, and HTTP/1.1 sends it with chunked encoding.
#[rstest]
#[case::http1(Transport::Http1)]
#[case::h2_alpn(Transport::H2Alpn)]
#[case::h2c(Transport::H2c)]
#[cfg_attr(feature = "aws-core", case::aws(Transport::Aws))]
#[cfg_attr(feature = "aws-core", case::aws_service(Transport::AwsService))]
#[cfg_attr(
    feature = "aws-core",
    case::aws_service_h2_alpn(Transport::AwsServiceH2Alpn)
)]
#[tokio::test]
async fn streams_data_frames_before_the_body_ends(#[case] transport: Transport) {
    let (server, mut received) = transport.spawn_server().await;
    let (frames, body) = channel_body();
    let response = transport.spawn(Request::post(transport.uri(&server)).body(body).unwrap());

    frames.send(Ok(data(b"first"))).unwrap();
    let Received { request, respond } = next_request(&mut received).await;
    let (parts, mut body) = request.into_parts();
    assert_eq!(parts.version, transport.version());
    assert!(!parts.headers.contains_key(header::CONTENT_LENGTH));
    assert_eq!(
        parts
            .headers
            .get(header::TRANSFER_ENCODING)
            .map(HeaderValue::as_bytes),
        (transport.version() == Version::HTTP_11).then_some(b"chunked".as_slice())
    );
    assert_eq!(next_data(&mut body).await, "first");

    frames.send(Ok(data(b"second"))).unwrap();
    assert_eq!(next_data(&mut body).await, "second");

    drop(frames);
    let rest = timeout(REQUEST_TIMEOUT, body.collect())
        .await
        .expect("the request body ends")
        .expect("the request body ends cleanly");
    assert!(rest.trailers().is_none());
    assert!(rest.to_bytes().is_empty());
    drop(respond);
    assert_eq!(status(response).await, Ok(StatusCode::OK));
}

/// Trailers that follow the data reach the server. HTTP/1.1 sends the trailer fields that the
/// `Trailer` request header declares.
#[rstest]
#[case::http1(Transport::Http1)]
#[case::h2_alpn(Transport::H2Alpn)]
#[case::h2c(Transport::H2c)]
#[cfg_attr(feature = "aws-core", case::aws(Transport::Aws))]
#[cfg_attr(feature = "aws-core", case::aws_service(Transport::AwsService))]
#[cfg_attr(
    feature = "aws-core",
    case::aws_service_h2_alpn(Transport::AwsServiceH2Alpn)
)]
#[tokio::test]
async fn sends_trailers_after_the_data(#[case] transport: Transport) {
    let (server, mut received) = transport.spawn_server().await;
    let mut trailers = HeaderMap::new();
    trailers.insert(CHECKSUM_TRAILER, HeaderValue::from_static("abc"));
    let frames: Vec<BodyFrame> = vec![Ok(data(b"data")), Ok(Frame::trailers(trailers.clone()))];
    let body = StreamBody::new(stream::iter(frames));
    let request = Request::post(transport.uri(&server))
        .header(header::TRAILER, CHECKSUM_TRAILER)
        .body(body)
        .unwrap();
    let response = transport.spawn(request);

    let Received { request, respond } = next_request(&mut received).await;
    let body = timeout(REQUEST_TIMEOUT, request.into_body().collect())
        .await
        .expect("the request body completes")
        .expect("the request body is valid");
    assert_eq!(body.trailers(), Some(&trailers));
    assert_eq!(body.to_bytes(), "data");
    drop(respond);
    assert_eq!(status(response).await, Ok(StatusCode::OK));
}

/// A body of known length is sent with its `Content-Length`, and an empty body ends the
/// stream with the request headers, without `Content-Length` or chunked encoding on a `GET`.
#[rstest]
#[case::http1(Transport::Http1)]
#[case::h2_alpn(Transport::H2Alpn)]
#[case::h2c(Transport::H2c)]
#[cfg_attr(feature = "aws-core", case::aws(Transport::Aws))]
#[cfg_attr(feature = "aws-core", case::aws_service(Transport::AwsService))]
#[cfg_attr(
    feature = "aws-core",
    case::aws_service_h2_alpn(Transport::AwsServiceH2Alpn)
)]
#[tokio::test]
async fn frames_bodies_by_their_size_hint(#[case] transport: Transport) {
    let (server, mut received) = transport.spawn_server().await;
    let response = transport.spawn(
        Request::post(transport.uri(&server))
            .body(full(b"12345"))
            .unwrap(),
    );
    let Received { request, respond } = next_request(&mut received).await;
    assert_eq!(request.headers()[header::CONTENT_LENGTH], "5");
    assert!(!request.headers().contains_key(header::TRANSFER_ENCODING));
    assert_eq!(request.body().size_hint().exact(), Some(5));
    let body = timeout(REQUEST_TIMEOUT, request.into_body().collect())
        .await
        .expect("the request body completes")
        .expect("the request body is valid");
    assert_eq!(body.to_bytes(), "12345");
    drop(respond);
    assert_eq!(status(response).await, Ok(StatusCode::OK));

    let response = transport.spawn(Request::get(transport.uri(&server)).body(empty()).unwrap());
    let Received { request, respond } = next_request(&mut received).await;
    assert!(request.body().is_end_stream());
    assert!(!request.headers().contains_key(header::CONTENT_LENGTH));
    assert!(!request.headers().contains_key(header::TRANSFER_ENCODING));
    drop(respond);
    assert_eq!(status(response).await, Ok(StatusCode::OK));
}

/// An error from the body fails the request, and the server sees an incomplete body, not a
/// complete one.
#[rstest]
#[case::http1(Transport::Http1)]
#[case::h2_alpn(Transport::H2Alpn)]
#[case::h2c(Transport::H2c)]
#[cfg_attr(feature = "aws-core", case::aws(Transport::Aws))]
#[cfg_attr(feature = "aws-core", case::aws_service(Transport::AwsService))]
#[cfg_attr(
    feature = "aws-core",
    case::aws_service_h2_alpn(Transport::AwsServiceH2Alpn)
)]
#[tokio::test]
async fn body_error_fails_the_request(#[case] transport: Transport) {
    let (server, mut received) = transport.spawn_server().await;
    let (frames, body) = channel_body();
    let response = transport.spawn(Request::post(transport.uri(&server)).body(body).unwrap());

    frames.send(Ok(data(b"partial"))).unwrap();
    let Received { request, respond } = next_request(&mut received).await;
    let mut body = request.into_body();
    assert_eq!(next_data(&mut body).await, "partial");

    frames.send(Err("the body failed".into())).unwrap();
    let frame = next_frame(&mut body).await;
    assert!(
        matches!(frame, Some(Err(_))),
        "the server must see an aborted body: {frame:?}"
    );
    drop(respond);
    let result = status(response).await;
    assert!(result.is_err(), "the request must fail: {result:?}");
}

/// Dropping a request before its body ends drops the body and aborts the request on the wire,
/// so the server does not see a complete body.
#[rstest]
#[case::http1(Transport::Http1)]
#[case::h2_alpn(Transport::H2Alpn)]
#[case::h2c(Transport::H2c)]
#[cfg_attr(feature = "aws-core", case::aws(Transport::Aws))]
#[cfg_attr(feature = "aws-core", case::aws_service(Transport::AwsService))]
#[cfg_attr(
    feature = "aws-core",
    case::aws_service_h2_alpn(Transport::AwsServiceH2Alpn)
)]
#[tokio::test]
async fn cancelled_request_releases_its_body(#[case] transport: Transport) {
    let (server, mut received) = transport.spawn_server().await;
    let (frames, body) = channel_body();
    let response = transport.spawn(Request::post(transport.uri(&server)).body(body).unwrap());

    frames.send(Ok(data(b"partial"))).unwrap();
    let Received { request, respond } = next_request(&mut received).await;
    let mut body = request.into_body();
    assert_eq!(next_data(&mut body).await, "partial");

    response.abort();
    timeout(REQUEST_TIMEOUT, frames.closed())
        .await
        .expect("the transport drops the body of a cancelled request");
    let frame = next_frame(&mut body).await;
    assert!(
        matches!(frame, Some(Err(_))),
        "the server must see an aborted body: {frame:?}"
    );
    drop(respond);
}

/// The transport stops polling a body while the server does not read it, and polls it again
/// once the server reads. HTTP/2 flow control bounds the bytes in flight to the stream window
/// and the send buffer; HTTP/1.1 to the socket buffers.
#[rstest]
#[case::http1(Transport::Http1, 64 << 20)]
#[case::h2_alpn(Transport::H2Alpn, 8 << 20)]
#[case::h2c(Transport::H2c, 8 << 20)]
#[cfg_attr(feature = "aws-core", case::aws(Transport::Aws, 64 << 20))]
#[cfg_attr(feature = "aws-core", case::aws_service(Transport::AwsService, 64 << 20))]
#[cfg_attr(
    feature = "aws-core",
    case::aws_service_h2_alpn(Transport::AwsServiceH2Alpn, 8 << 20)
)]
#[tokio::test]
async fn stops_polling_the_body_while_the_server_does_not_read(
    #[case] transport: Transport,
    #[case] max_bytes_in_flight: usize,
) {
    let (server, mut received) = transport.spawn_server().await;
    let polled = Arc::new(AtomicUsize::new(0));
    let chunk = Bytes::from(vec![0; 16 << 10]);
    let counter = Arc::clone(&polled);
    let body = StreamBody::new(stream::repeat_with(move || {
        counter.fetch_add(chunk.len(), Ordering::Relaxed);
        Ok::<_, Error>(Frame::data(chunk.clone()))
    }));
    let response = transport.spawn(Request::post(transport.uri(&server)).body(body).unwrap());

    let Received { request, respond } = next_request(&mut received).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let stalled = polled.load(Ordering::Relaxed);
    assert!(
        stalled <= max_bytes_in_flight,
        "the body was polled for {stalled} bytes while the server did not read"
    );

    let mut body = request.into_body();
    timeout(REQUEST_TIMEOUT, async {
        while polled.load(Ordering::Relaxed) <= stalled {
            body.frame()
                .await
                .expect("the body continues")
                .expect("the frame is valid");
        }
    })
    .await
    .expect("reading the body lets the transport poll it again");

    response.abort();
    drop(respond);
}
