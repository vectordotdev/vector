use std::{
    convert::Infallible,
    fmt::Display,
    future::Future,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use http_1::{HeaderMap, Method, Request, Response, StatusCode, Uri, Version, header};
use http_body_util::{BodyExt as _, Full};
use hyper::body::HttpBody as _;
use hyper_1::{body::Incoming, client::conn, service::service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto,
};
use rstest::rstest;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _, copy_bidirectional},
    net::TcpStream,
    task::{JoinHandle, JoinSet},
    time::timeout,
};
use vector_http::{ConnectionOptions, RequestBody, empty_body};

use super::{HttpClient as LegacyHttpClient, client_v1::HttpClient};
use crate::{
    config::ProxyConfig,
    tls::{
        MaybeTlsIncomingStream, MaybeTlsSettings, TEST_PEM_CA_PATH, TEST_PEM_CLIENT_CRT_PATH,
        TEST_PEM_CLIENT_KEY_PATH, TEST_PEM_CRT_PATH, TEST_PEM_KEY_PATH, TlsConfig,
        TlsEnableableConfig, TlsSettings,
    },
};

mod http2;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const INVALID_CA_PATH: &str = "tests/integration/http-client/data/certs/invalid-ca-cert.pem";
const PROXY_USERNAME: &str = "proxy-user";
const PROXY_PASSWORD: &str = "proxy-pass";
const PROXY_AUTHORIZATION: &str = "Basic cHJveHktdXNlcjpwcm94eS1wYXNz";

#[derive(Clone, Debug)]
struct RequestObservation {
    method: Method,
    uri: Uri,
    version: Version,
    headers: HeaderMap,
    /// The ALPN protocol negotiated on the request's TLS connection, if any.
    alpn: Option<Vec<u8>>,
    /// Identifies the connection that carried the request, in accept order.
    connection: usize,
}

struct TestServer {
    addr: SocketAddr,
    observations: Arc<Mutex<Vec<RequestObservation>>>,
    task: JoinHandle<()>,
}

impl TestServer {
    fn http_uri(&self) -> String {
        format!("http://localhost:{}", self.addr.port())
    }

    fn https_uri(&self) -> String {
        format!("https://localhost:{}", self.addr.port())
    }

    fn http_ip_uri(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn https_ip_uri(&self) -> String {
        format!("https://{}", self.addr)
    }

    fn observations(&self) -> Vec<RequestObservation> {
        self.observations.lock().unwrap().clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Debug)]
struct TestResponse {
    status: u16,
    body: Vec<u8>,
}

#[async_trait]
trait TestClient: Send + Sync {
    async fn get(&self, uri: &str) -> Result<TestResponse, String>;
    async fn get_with_headers(
        &self,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> Result<TestResponse, String>;
}

#[derive(Clone, Copy, Debug)]
enum ClientVersion {
    Legacy,
    V1,
}

impl ClientVersion {
    fn build(
        self,
        tls: MaybeTlsSettings,
        proxy: &ProxyConfig,
    ) -> Result<Box<dyn TestClient>, String> {
        match self {
            Self::Legacy => Ok(Box::new(
                LegacyHttpClient::new(tls, proxy).map_err(|error| error.to_string())?,
            )),
            Self::V1 => Ok(Box::new(
                HttpClient::new(tls, proxy).map_err(|error| error.to_string())?,
            )),
        }
    }
}

#[async_trait]
impl TestClient for LegacyHttpClient {
    async fn get(&self, uri: &str) -> Result<TestResponse, String> {
        self.get_with_headers(uri, &[]).await
    }

    async fn get_with_headers(
        &self,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> Result<TestResponse, String> {
        let mut request = http::Request::get(uri)
            .body(hyper::Body::empty())
            .map_err(|error| error.to_string())?;
        for &(name, value) in headers {
            request.headers_mut().insert(
                http::header::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|error| error.to_string())?,
                value
                    .parse()
                    .map_err(|error: http::header::InvalidHeaderValue| error.to_string())?,
            );
        }
        let response = timeout(REQUEST_TIMEOUT, self.send(request))
            .await
            .map_err(|_| "request timed out".to_owned())?
            .map_err(|error| error.to_string())?;
        let status = response.status();
        let body = timeout(REQUEST_TIMEOUT, response.into_body().collect())
            .await
            .map_err(|_| "response body timed out".to_owned())?
            .map_err(|error| error.to_string())?
            .to_bytes();
        Ok(TestResponse {
            status: status.as_u16(),
            body: body.to_vec(),
        })
    }
}

#[async_trait]
impl TestClient for HttpClient {
    async fn get(&self, uri: &str) -> Result<TestResponse, String> {
        self.get_with_headers(uri, &[]).await
    }

    async fn get_with_headers(
        &self,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> Result<TestResponse, String> {
        native_get(uri, headers, |request| self.send(request)).await
    }
}

/// Sends a `GET` request with `headers` through a native HTTP 1 client and collects the response.
async fn native_get<F, E>(
    uri: &str,
    headers: &[(&str, &str)],
    send: impl FnOnce(Request<RequestBody>) -> F,
) -> Result<TestResponse, String>
where
    F: Future<Output = Result<Response<Incoming>, E>>,
    E: Display,
{
    let mut request = Request::get(uri)
        .body(empty_body())
        .map_err(|error| error.to_string())?;
    for &(name, value) in headers {
        request.headers_mut().insert(
            header::HeaderName::from_bytes(name.as_bytes()).map_err(|error| error.to_string())?,
            value
                .parse()
                .map_err(|error: header::InvalidHeaderValue| error.to_string())?,
        );
    }
    let response = timeout(REQUEST_TIMEOUT, send(request))
        .await
        .map_err(|_| "request timed out".to_owned())?
        .map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let body = timeout(REQUEST_TIMEOUT, response.into_body().collect())
        .await
        .map_err(|_| "response body timed out".to_owned())?
        .map_err(|error| error.to_string())?
        .to_bytes();
    Ok(TestResponse {
        status,
        body: body.to_vec(),
    })
}

fn no_tls() -> MaybeTlsSettings {
    MaybeTlsSettings::from_config(None, false).unwrap()
}

// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::needless_pass_by_value,
    reason = "Preserve ownership and drop timing"
)]
fn client_tls(config: TlsConfig) -> MaybeTlsSettings {
    TlsSettings::from_options(Some(&config)).unwrap().into()
}

fn trusted_client_tls() -> MaybeTlsSettings {
    client_tls(TlsConfig {
        ca_file: Some(TEST_PEM_CA_PATH.into()),
        ..Default::default()
    })
}

fn server_tls(require_client_certificate: bool) -> MaybeTlsSettings {
    server_tls_with_alpn(require_client_certificate, None)
}

fn server_tls_with_alpn(
    require_client_certificate: bool,
    alpn_protocols: Option<Vec<String>>,
) -> MaybeTlsSettings {
    MaybeTlsSettings::from_config(
        Some(&TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                verify_certificate: require_client_certificate.then_some(true),
                ca_file: require_client_certificate.then(|| TEST_PEM_CA_PATH.into()),
                crt_file: Some(TEST_PEM_CRT_PATH.into()),
                key_file: Some(TEST_PEM_KEY_PATH.into()),
                alpn_protocols,
                ..Default::default()
            },
        }),
        true,
    )
    .unwrap()
}

type ServerResponse = Response<Full<Bytes>>;

/// Spawns a server that accepts HTTP/1.1, with upgrades, and HTTP/2 with prior knowledge on each
/// connection. It records every request before it hands the request to `handler`.
async fn spawn_server<H, F>(tls: MaybeTlsSettings, handler: H) -> TestServer
where
    H: Fn(Request<Incoming>) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = ServerResponse> + Send + 'static,
{
    let addr = "127.0.0.1:0".parse().unwrap();
    let mut listener = tls.bind(&addr).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let observations = Arc::new(Mutex::new(Vec::new()));
    let server_observations = Arc::clone(&observations);

    let task = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        let mut next_connection = 0;
        loop {
            let Ok(mut stream) = listener.accept().await else {
                continue;
            };
            while connections.try_join_next().is_some() {}
            let connection = next_connection;
            next_connection += 1;
            let observations = Arc::clone(&server_observations);
            let handler = handler.clone();
            connections.spawn(async move {
                if stream.handshake().await.is_err() {
                    return;
                }
                let alpn = negotiated_alpn(&stream);
                let service = service_fn(move |request: Request<Incoming>| {
                    observations.lock().unwrap().push(RequestObservation {
                        method: request.method().clone(),
                        uri: request.uri().clone(),
                        version: request.version(),
                        headers: request.headers().clone(),
                        alpn: alpn.clone(),
                        connection,
                    });
                    let response = handler(request);
                    async move { Ok::<_, Infallible>(response.await) }
                });
                if let Err(error) = auto::Builder::new(TokioExecutor::new())
                    .serve_connection_with_upgrades(TokioIo::new(stream), service)
                    .await
                {
                    tracing::debug!(message = "Test server connection closed.", %error);
                }
            });
        }
    });

    TestServer {
        addr,
        observations,
        task,
    }
}

fn negotiated_alpn(stream: &MaybeTlsIncomingStream<TcpStream>) -> Option<Vec<u8>> {
    stream
        .ssl_stream()
        .and_then(|stream| stream.ssl().selected_alpn_protocol())
        .map(<[u8]>::to_vec)
}

async fn spawn_origin(tls: MaybeTlsSettings) -> TestServer {
    spawn_server(tls, |_| async {
        Response::new(Full::new(Bytes::from_static(b"origin response")))
    })
    .await
}

async fn spawn_proxy(tls: MaybeTlsSettings, require_authentication: bool) -> TestServer {
    spawn_proxy_with_connect_status(tls, require_authentication, StatusCode::OK).await
}

/// Spawns a proxy that tunnels `CONNECT` requests and forwards absolute-form requests to their
/// authority over the HTTP version of the request.
async fn spawn_proxy_with_connect_status(
    tls: MaybeTlsSettings,
    require_authentication: bool,
    connect_status: StatusCode,
) -> TestServer {
    spawn_server(tls, move |request| {
        proxy_request(request, require_authentication, connect_status)
    })
    .await
}

async fn proxy_request(
    mut request: Request<Incoming>,
    require_authentication: bool,
    connect_status: StatusCode,
) -> ServerResponse {
    if require_authentication && !has_proxy_authorization(request.headers()) {
        return empty_response(StatusCode::PROXY_AUTHENTICATION_REQUIRED);
    }

    if request.method() == Method::CONNECT {
        let Some(authority) = request.uri().authority().cloned() else {
            return empty_response(StatusCode::BAD_GATEWAY);
        };
        tokio::spawn(async move {
            let Ok(upgraded) = hyper_1::upgrade::on(&mut request).await else {
                return;
            };
            let Ok(mut upstream) = TcpStream::connect(authority.as_str()).await else {
                return;
            };
            if let Err(error) = copy_bidirectional(&mut TokioIo::new(upgraded), &mut upstream).await
            {
                tracing::debug!(message = "Proxy tunnel closed.", %error);
            }
        });
        return empty_response(connect_status);
    }

    forward(request).await
}

fn has_proxy_authorization(headers: &HeaderMap) -> bool {
    headers
        .get(header::PROXY_AUTHORIZATION)
        .is_some_and(|value| value.as_bytes() == PROXY_AUTHORIZATION.as_bytes())
}

async fn forward(mut request: Request<Incoming>) -> ServerResponse {
    let Some(authority) = request.uri().authority().cloned() else {
        return empty_response(StatusCode::BAD_GATEWAY);
    };
    let Ok(stream) = TcpStream::connect(authority.as_str()).await else {
        return empty_response(StatusCode::BAD_GATEWAY);
    };
    let stream = TokioIo::new(stream);
    request.headers_mut().remove(header::PROXY_AUTHORIZATION);

    let response = if request.version() == Version::HTTP_2 {
        // HTTP/2 carries the destination in `:scheme` and `:authority`, so the absolute URI stays.
        let Ok((mut sender, connection)) =
            conn::http2::handshake(TokioExecutor::new(), stream).await
        else {
            return empty_response(StatusCode::BAD_GATEWAY);
        };
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::debug!(message = "Proxy upstream connection closed.", %error);
            }
        });
        sender.send_request(request).await
    } else {
        let Ok((mut sender, connection)) = conn::http1::handshake(stream).await else {
            return empty_response(StatusCode::BAD_GATEWAY);
        };
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::debug!(message = "Proxy upstream connection closed.", %error);
            }
        });
        let path = request
            .uri()
            .path_and_query()
            .map_or("/", |path| path.as_str())
            .parse()
            .unwrap();
        *request.uri_mut() = path;
        sender.send_request(request).await
    };

    let Ok(response) = response else {
        return empty_response(StatusCode::BAD_GATEWAY);
    };
    let (parts, body) = response.into_parts();
    match body.collect().await {
        Ok(body) => Response::from_parts(parts, Full::new(body.to_bytes())),
        Err(_) => empty_response(StatusCode::BAD_GATEWAY),
    }
}

fn empty_response(status: StatusCode) -> ServerResponse {
    Response::builder()
        .status(status)
        .body(Full::default())
        .unwrap()
}

fn proxy_config(proxy: &TestServer, tls: bool, auth: bool) -> ProxyConfig {
    let scheme = if tls { "https" } else { "http" };
    let credentials = if auth {
        format!("{PROXY_USERNAME}:{PROXY_PASSWORD}@")
    } else {
        String::new()
    };
    let url = format!("{scheme}://{credentials}localhost:{}", proxy.addr.port());
    ProxyConfig {
        http: Some(url.clone()),
        https: Some(url),
        ..Default::default()
    }
}

// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::needless_pass_by_value,
    reason = "Preserve ownership and drop timing"
)]
fn assert_success(response: TestResponse) {
    assert_eq!(response.status, StatusCode::OK.as_u16());
    assert_eq!(response.body, b"origin response");
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn direct_http(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(no_tls()).await;
    let client = client_version
        .build(no_tls(), &ProxyConfig::default())
        .unwrap();

    assert_success(client.get(&origin.http_uri()).await.unwrap());
    assert_eq!(origin.observations().len(), 1);
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn direct_https_with_trusted_ca(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(server_tls(false)).await;
    let client = client_version
        .build(trusted_client_tls(), &ProxyConfig::default())
        .unwrap();

    assert_success(client.get(&origin.https_uri()).await.unwrap());
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn direct_https_rejects_untrusted_ca(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(server_tls(false)).await;
    let client = client_version
        .build(
            client_tls(TlsConfig {
                ca_file: Some(INVALID_CA_PATH.into()),
                ..Default::default()
            }),
            &ProxyConfig::default(),
        )
        .unwrap();

    client
        .get(&origin.https_uri())
        .await
        .expect_err("an untrusted server certificate must fail");
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn direct_https_honors_server_name(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(server_tls(false)).await;
    let without_override = client_version
        .build(trusted_client_tls(), &ProxyConfig::default())
        .unwrap();
    without_override
        .get(&origin.https_ip_uri())
        .await
        .expect_err("the server certificate does not cover its IP address");

    let with_override = client_version
        .build(
            client_tls(TlsConfig {
                ca_file: Some(TEST_PEM_CA_PATH.into()),
                server_name: Some("localhost".to_owned()),
                ..Default::default()
            }),
            &ProxyConfig::default(),
        )
        .unwrap();
    assert_success(with_override.get(&origin.https_ip_uri()).await.unwrap());
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn direct_https_supports_mtls(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(server_tls(true)).await;
    let without_identity = client_version
        .build(trusted_client_tls(), &ProxyConfig::default())
        .unwrap();
    without_identity
        .get(&origin.https_uri())
        .await
        .expect_err("the server requires a client certificate");

    let with_identity = client_version
        .build(
            client_tls(TlsConfig {
                ca_file: Some(TEST_PEM_CA_PATH.into()),
                crt_file: Some(TEST_PEM_CLIENT_CRT_PATH.into()),
                key_file: Some(TEST_PEM_CLIENT_KEY_PATH.into()),
                ..Default::default()
            }),
            &ProxyConfig::default(),
        )
        .unwrap();
    assert_success(with_identity.get(&origin.https_uri()).await.unwrap());
}

fn mtls_client_tls() -> MaybeTlsSettings {
    client_tls(TlsConfig {
        ca_file: Some(TEST_PEM_CA_PATH.into()),
        crt_file: Some(TEST_PEM_CLIENT_CRT_PATH.into()),
        key_file: Some(TEST_PEM_CLIENT_KEY_PATH.into()),
        ..Default::default()
    })
}

#[tokio::test]
async fn v1_reload_tls_applies_to_clones() {
    let origin = spawn_origin(server_tls(true)).await;
    let client = HttpClient::new(trusted_client_tls(), &ProxyConfig::default()).unwrap();
    client
        .get(&origin.https_uri())
        .await
        .expect_err("the server requires a client certificate");

    let handle = client.clone().tls_reload_handle();
    assert!(handle.reload(mtls_client_tls()).unwrap());

    assert_success(client.get(&origin.https_uri()).await.unwrap());
}

#[tokio::test]
async fn v1_tls_reload_handle_does_not_keep_client_alive() {
    let client = HttpClient::new(trusted_client_tls(), &ProxyConfig::default()).unwrap();
    let handle = client.tls_reload_handle();
    let clone = client.clone();
    drop(client);
    assert!(!handle.is_closed());

    drop(clone);
    assert!(handle.is_closed());
    assert!(!handle.reload(mtls_client_tls()).unwrap());
}

#[tokio::test]
async fn v1_reload_tls_does_not_reuse_pooled_connections() {
    let origin = spawn_origin(server_tls(false)).await;
    let client = HttpClient::new(trusted_client_tls(), &ProxyConfig::default()).unwrap();
    assert_success(client.get(&origin.https_uri()).await.unwrap());

    let reloaded = client
        .tls_reload_handle()
        .reload(client_tls(TlsConfig {
            ca_file: Some(INVALID_CA_PATH.into()),
            ..Default::default()
        }))
        .unwrap();
    assert!(reloaded);

    // The pooled connection was verified against the trusted CA, so only a new connection made
    // with the reloaded settings rejects the server.
    client
        .get(&origin.https_uri())
        .await
        .expect_err("requests after a reload must use the new TLS settings");
}

#[rstest]
#[case::without_max_age(None, 1)]
#[case::with_max_age(Some(Duration::from_millis(500)), 2)]
#[tokio::test]
async fn v1_max_connection_age_recycles_connections(
    #[case] max_connection_age: Option<Duration>,
    #[case] expected_connections: usize,
) {
    let origin = spawn_origin(no_tls()).await;
    let mut options = ConnectionOptions::default();
    options.max_connection_age = max_connection_age;
    let client = HttpClient::new_with_options(no_tls(), &ProxyConfig::default(), options).unwrap();

    assert_success(client.get(&origin.http_uri()).await.unwrap());
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_success(client.get(&origin.http_uri()).await.unwrap());

    let mut connections: Vec<_> = origin
        .observations()
        .iter()
        .map(|observation| observation.connection)
        .collect();
    connections.dedup();
    assert_eq!(connections.len(), expected_connections);
}

#[tokio::test]
async fn v1_max_connection_age_fails_truncated_close_delimited_body() {
    // A close-delimited response (no `Content-Length` or `Transfer-Encoding`) whose body is
    // still streaming when the connection reaches its maximum age.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request).await.unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\npartial ")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        // The client may already have dropped the connection.
        stream.write_all(b"rest of the body").await.ok();
    });

    let mut options = ConnectionOptions::default();
    options.max_connection_age = Some(Duration::from_millis(300));
    let client = HttpClient::new_with_options(no_tls(), &ProxyConfig::default(), options).unwrap();

    let response = client
        .send(
            Request::get(format!("http://{addr}/"))
                .body(empty_body())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = timeout(REQUEST_TIMEOUT, response.into_body().collect())
        .await
        .expect("response body timed out");
    assert!(
        body.is_err(),
        "a body cut off by the maximum age must fail, not complete truncated"
    );
    server.abort();
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn http_via_authenticated_proxy(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(no_tls()).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let client = client_version
        .build(no_tls(), &proxy_config(&proxy, false, true))
        .unwrap();

    assert_success(
        client
            .get_with_headers(
                &origin.http_uri(),
                &[("authorization", "Bearer destination-token")],
            )
            .await
            .unwrap(),
    );
    let proxy_observations = proxy.observations();
    assert_eq!(proxy_observations.len(), 1);
    assert_eq!(proxy_observations[0].method, Method::GET);
    assert_eq!(
        proxy_observations[0].uri.to_string(),
        format!("{}/", origin.http_uri())
    );
    assert!(has_proxy_authorization(&proxy_observations[0].headers));
    let origin_headers = &origin.observations()[0].headers;
    assert!(!origin_headers.contains_key(header::PROXY_AUTHORIZATION));
    assert_eq!(
        origin_headers.get(header::AUTHORIZATION).unwrap(),
        "Bearer destination-token"
    );
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn http_proxy_credentials_do_not_reach_origin(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(no_tls()).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let client = client_version
        .build(no_tls(), &proxy_config(&proxy, false, true))
        .unwrap();

    assert_success(client.get(&origin.http_uri()).await.unwrap());

    let proxy_observations = proxy.observations();
    assert_eq!(proxy_observations.len(), 1);
    assert!(has_proxy_authorization(&proxy_observations[0].headers));
    assert!(
        !proxy_observations[0]
            .headers
            .contains_key(header::AUTHORIZATION)
    );

    let origin_headers = &origin.observations()[0].headers;
    assert!(!origin_headers.contains_key(header::PROXY_AUTHORIZATION));
    assert!(!origin_headers.contains_key(header::AUTHORIZATION));
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn https_via_authenticated_connect_proxy(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(server_tls(false)).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let client = client_version
        .build(trusted_client_tls(), &proxy_config(&proxy, false, true))
        .unwrap();

    assert_success(
        client
            .get_with_headers(
                &origin.https_uri(),
                &[("authorization", "Bearer destination-token")],
            )
            .await
            .unwrap(),
    );
    let proxy_observations = proxy.observations();
    assert_eq!(proxy_observations.len(), 1);
    assert_eq!(proxy_observations[0].method, Method::CONNECT);
    assert!(has_proxy_authorization(&proxy_observations[0].headers));
    assert!(
        !proxy_observations[0]
            .headers
            .contains_key(header::AUTHORIZATION)
    );
    let origin_headers = &origin.observations()[0].headers;
    assert!(!origin_headers.contains_key(header::PROXY_AUTHORIZATION));
    assert_eq!(
        origin_headers.get(header::AUTHORIZATION).unwrap(),
        "Bearer destination-token"
    );
}

#[tokio::test]
async fn v1_accepts_any_successful_connect_status() {
    let origin = spawn_origin(server_tls(false)).await;
    let proxy = spawn_proxy_with_connect_status(no_tls(), false, StatusCode::CREATED).await;
    let client = ClientVersion::V1
        .build(trusted_client_tls(), &proxy_config(&proxy, false, false))
        .unwrap();

    assert_success(client.get(&origin.https_uri()).await.unwrap());
}

#[tokio::test]
async fn v1_tls_proxy_uses_http1_alpn() {
    let origin = spawn_origin(server_tls(false)).await;
    let proxy = spawn_proxy(
        server_tls_with_alpn(false, Some(vec!["h2".to_owned(), "http/1.1".to_owned()])),
        false,
    )
    .await;
    let client = ClientVersion::V1
        .build(
            client_tls(TlsConfig {
                ca_file: Some(TEST_PEM_CA_PATH.into()),
                alpn_protocols: Some(vec!["h2".to_owned(), "http/1.1".to_owned()]),
                ..Default::default()
            }),
            &proxy_config(&proxy, true, false),
        )
        .unwrap();

    assert_success(client.get(&origin.https_uri()).await.unwrap());
    assert_eq!(
        proxy.observations()[0].alpn.as_deref(),
        Some(b"http/1.1".as_slice())
    );
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn no_proxy_bypasses_proxy(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(no_tls()).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let mut config = proxy_config(&proxy, false, false);
    config.no_proxy = "127.0.0.1".into();
    let client = client_version.build(no_tls(), &config).unwrap();

    assert_success(client.get(&origin.http_ip_uri()).await.unwrap());
    assert!(proxy.observations().is_empty());
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn disabled_proxy_is_bypassed(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(no_tls()).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let mut config = proxy_config(&proxy, false, false);
    config.enabled = false;
    let client = client_version.build(no_tls(), &config).unwrap();

    assert_success(client.get(&origin.http_uri()).await.unwrap());
    assert!(proxy.observations().is_empty());
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn http_via_tls_proxy(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(no_tls()).await;
    let proxy = spawn_proxy(server_tls(false), false).await;
    let client = client_version
        .build(trusted_client_tls(), &proxy_config(&proxy, true, false))
        .unwrap();

    assert_success(client.get(&origin.http_uri()).await.unwrap());
    assert_eq!(proxy.observations().len(), 1);
}

#[rstest]
#[case::legacy(ClientVersion::Legacy)]
#[case::v1(ClientVersion::V1)]
#[tokio::test]
async fn https_via_tls_connect_proxy(#[case] client_version: ClientVersion) {
    let origin = spawn_origin(server_tls(false)).await;
    let proxy = spawn_proxy(server_tls(false), false).await;
    let client = client_version
        .build(trusted_client_tls(), &proxy_config(&proxy, true, false))
        .unwrap();

    assert_success(client.get(&origin.https_uri()).await.unwrap());
    let observations = proxy.observations();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].method, Method::CONNECT);
}
