use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use http::{HeaderMap, Method, Request, Response, StatusCode, Uri, header};
use hyper::{Body, body::HttpBody as _, client::conn, server::conn::http1, service::service_fn};
use tokio::{io::copy_bidirectional, net::TcpStream, task::JoinHandle, time::timeout};

use super::HttpClient;
use crate::{
    config::ProxyConfig,
    tls::{
        MaybeTlsSettings, TEST_PEM_CA_PATH, TEST_PEM_CLIENT_CRT_PATH, TEST_PEM_CLIENT_KEY_PATH,
        TEST_PEM_CRT_PATH, TEST_PEM_KEY_PATH, TlsConfig, TlsEnableableConfig, TlsSettings,
    },
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const INVALID_CA_PATH: &str = "tests/integration/http-client/data/certs/invalid-ca-cert.pem";
const PROXY_USERNAME: &str = "proxy-user";
const PROXY_PASSWORD: &str = "proxy-pass";
const PROXY_AUTHORIZATION: &str = "Basic cHJveHktdXNlcjpwcm94eS1wYXNz";

#[derive(Clone, Debug)]
struct RequestObservation {
    method: Method,
    uri: Uri,
    headers: HeaderMap,
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
    status: StatusCode,
    body: Vec<u8>,
}

#[async_trait]
trait TestClient: Send + Sync {
    async fn get(&self, uri: &str) -> Result<TestResponse, String>;
    async fn get_with_headers(&self, uri: &str, headers: HeaderMap)
    -> Result<TestResponse, String>;
}

trait TestClientFactory: Copy {
    type Client: TestClient;

    fn build(self, tls: MaybeTlsSettings, proxy: &ProxyConfig) -> Result<Self::Client, String>;
}

#[derive(Clone, Copy)]
struct LegacyClientFactory;

impl TestClientFactory for LegacyClientFactory {
    type Client = HttpClient;

    fn build(self, tls: MaybeTlsSettings, proxy: &ProxyConfig) -> Result<Self::Client, String> {
        HttpClient::new(tls, proxy).map_err(|error| error.to_string())
    }
}

#[async_trait]
impl TestClient for HttpClient {
    async fn get(&self, uri: &str) -> Result<TestResponse, String> {
        self.get_with_headers(uri, HeaderMap::new()).await
    }

    async fn get_with_headers(
        &self,
        uri: &str,
        headers: HeaderMap,
    ) -> Result<TestResponse, String> {
        let mut request = Request::get(uri)
            .body(Body::empty())
            .map_err(|error| error.to_string())?;
        *request.headers_mut() = headers;
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
            status,
            body: body.to_vec(),
        })
    }
}

fn no_tls() -> MaybeTlsSettings {
    MaybeTlsSettings::from_config(None, false).unwrap()
}

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
    MaybeTlsSettings::from_config(
        Some(&TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                verify_certificate: require_client_certificate.then_some(true),
                ca_file: require_client_certificate.then(|| TEST_PEM_CA_PATH.into()),
                crt_file: Some(TEST_PEM_CRT_PATH.into()),
                key_file: Some(TEST_PEM_KEY_PATH.into()),
                ..Default::default()
            },
        }),
        true,
    )
    .unwrap()
}

async fn spawn_origin(tls: MaybeTlsSettings) -> TestServer {
    let addr = "127.0.0.1:0".parse().unwrap();
    let mut listener = tls.bind(&addr).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let observations = Arc::new(Mutex::new(Vec::new()));
    let server_observations = Arc::clone(&observations);

    let task = tokio::spawn(async move {
        loop {
            let Ok(stream) = listener.accept().await else {
                continue;
            };
            let observations = Arc::clone(&server_observations);
            tokio::spawn(async move {
                let service = service_fn(move |request: Request<Body>| {
                    observations.lock().unwrap().push(RequestObservation {
                        method: request.method().clone(),
                        uri: request.uri().clone(),
                        headers: request.headers().clone(),
                    });
                    async move { Ok::<_, Infallible>(Response::new(Body::from("origin response"))) }
                });
                if let Err(error) = http1::Builder::new()
                    .serve_connection(stream, service)
                    .await
                {
                    tracing::debug!(message = "Origin connection closed.", %error);
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

async fn spawn_proxy(tls: MaybeTlsSettings, require_authentication: bool) -> TestServer {
    let addr = "127.0.0.1:0".parse().unwrap();
    let mut listener = tls.bind(&addr).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let observations = Arc::new(Mutex::new(Vec::new()));
    let server_observations = Arc::clone(&observations);

    let task = tokio::spawn(async move {
        loop {
            let Ok(stream) = listener.accept().await else {
                continue;
            };
            let observations = Arc::clone(&server_observations);
            tokio::spawn(async move {
                let service = service_fn(move |request: Request<Body>| {
                    let observations = Arc::clone(&observations);
                    async move {
                        Ok::<_, Infallible>(
                            proxy_request(request, observations, require_authentication).await,
                        )
                    }
                });
                if let Err(error) = http1::Builder::new()
                    .serve_connection(stream, service)
                    .with_upgrades()
                    .await
                {
                    tracing::debug!(message = "Proxy connection closed.", %error);
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

async fn proxy_request(
    mut request: Request<Body>,
    observations: Arc<Mutex<Vec<RequestObservation>>>,
    require_authentication: bool,
) -> Response<Body> {
    observations.lock().unwrap().push(RequestObservation {
        method: request.method().clone(),
        uri: request.uri().clone(),
        headers: request.headers().clone(),
    });

    if require_authentication && !has_proxy_authorization(request.headers()) {
        return Response::builder()
            .status(StatusCode::PROXY_AUTHENTICATION_REQUIRED)
            .body(Body::empty())
            .unwrap();
    }

    if request.method() == Method::CONNECT {
        let Some(authority) = request.uri().authority().cloned() else {
            return bad_gateway();
        };
        tokio::spawn(async move {
            let Ok(mut upgraded) = hyper::upgrade::on(&mut request).await else {
                return;
            };
            let Ok(mut upstream) = TcpStream::connect(authority.as_str()).await else {
                return;
            };
            if let Err(error) = copy_bidirectional(&mut upgraded, &mut upstream).await {
                tracing::debug!(message = "Proxy tunnel closed.", %error);
            }
        });
        return Response::new(Body::empty());
    }

    forward_http(request).await
}

fn has_proxy_authorization(headers: &HeaderMap) -> bool {
    headers
        .get(header::PROXY_AUTHORIZATION)
        .is_some_and(|value| value.as_bytes() == PROXY_AUTHORIZATION.as_bytes())
}

async fn forward_http(mut request: Request<Body>) -> Response<Body> {
    let Some(authority) = request.uri().authority().cloned() else {
        return bad_gateway();
    };
    let Ok(stream) = TcpStream::connect(authority.as_str()).await else {
        return bad_gateway();
    };
    let Ok((mut sender, connection)) = conn::http1::handshake(stream).await else {
        return bad_gateway();
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
    request.headers_mut().remove(header::PROXY_AUTHORIZATION);

    sender
        .send_request(request)
        .await
        .unwrap_or_else(|_| bad_gateway())
}

fn bad_gateway() -> Response<Body> {
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .body(Body::empty())
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

fn assert_success(response: TestResponse) {
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body, b"origin response");
}

async fn direct_http<F: TestClientFactory>(factory: F) {
    let origin = spawn_origin(no_tls()).await;
    let client = factory.build(no_tls(), &ProxyConfig::default()).unwrap();

    assert_success(client.get(&origin.http_uri()).await.unwrap());
    assert_eq!(origin.observations().len(), 1);
}

async fn direct_https_with_trusted_ca<F: TestClientFactory>(factory: F) {
    let origin = spawn_origin(server_tls(false)).await;
    let client = factory
        .build(trusted_client_tls(), &ProxyConfig::default())
        .unwrap();

    assert_success(client.get(&origin.https_uri()).await.unwrap());
}

async fn direct_https_rejects_untrusted_ca<F: TestClientFactory>(factory: F) {
    let origin = spawn_origin(server_tls(false)).await;
    let client = factory
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

async fn direct_https_honors_server_name<F: TestClientFactory>(factory: F) {
    let origin = spawn_origin(server_tls(false)).await;
    let without_override = factory
        .build(trusted_client_tls(), &ProxyConfig::default())
        .unwrap();
    without_override
        .get(&origin.https_ip_uri())
        .await
        .expect_err("the server certificate does not cover its IP address");

    let with_override = factory
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

async fn direct_https_supports_mtls<F: TestClientFactory>(factory: F) {
    let origin = spawn_origin(server_tls(true)).await;
    let without_identity = factory
        .build(trusted_client_tls(), &ProxyConfig::default())
        .unwrap();
    without_identity
        .get(&origin.https_uri())
        .await
        .expect_err("the server requires a client certificate");

    let with_identity = factory
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

async fn http_via_authenticated_proxy<F: TestClientFactory>(factory: F) {
    let origin = spawn_origin(no_tls()).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let client = factory
        .build(no_tls(), &proxy_config(&proxy, false, true))
        .unwrap();

    let mut destination_headers = HeaderMap::new();
    destination_headers.insert(
        header::AUTHORIZATION,
        "Bearer destination-token".parse().unwrap(),
    );
    assert_success(
        client
            .get_with_headers(&origin.http_uri(), destination_headers)
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

async fn https_via_authenticated_connect_proxy<F: TestClientFactory>(factory: F) {
    let origin = spawn_origin(server_tls(false)).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let client = factory
        .build(trusted_client_tls(), &proxy_config(&proxy, false, true))
        .unwrap();

    assert_success(client.get(&origin.https_uri()).await.unwrap());
    let proxy_observations = proxy.observations();
    assert_eq!(proxy_observations.len(), 1);
    assert_eq!(proxy_observations[0].method, Method::CONNECT);
    assert!(has_proxy_authorization(&proxy_observations[0].headers));
    let origin_headers = &origin.observations()[0].headers;
    assert!(!origin_headers.contains_key(header::PROXY_AUTHORIZATION));
    assert!(!origin_headers.contains_key(header::AUTHORIZATION));
}

async fn no_proxy_bypasses_proxy<F: TestClientFactory>(factory: F) {
    let origin = spawn_origin(no_tls()).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let mut config = proxy_config(&proxy, false, false);
    config.no_proxy = "127.0.0.1".into();
    let client = factory.build(no_tls(), &config).unwrap();

    assert_success(client.get(&origin.http_ip_uri()).await.unwrap());
    assert!(proxy.observations().is_empty());
}

async fn disabled_proxy_is_bypassed<F: TestClientFactory>(factory: F) {
    let origin = spawn_origin(no_tls()).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let mut config = proxy_config(&proxy, false, false);
    config.enabled = false;
    let client = factory.build(no_tls(), &config).unwrap();

    assert_success(client.get(&origin.http_uri()).await.unwrap());
    assert!(proxy.observations().is_empty());
}

async fn http_via_tls_proxy<F: TestClientFactory>(factory: F) {
    let origin = spawn_origin(no_tls()).await;
    let proxy = spawn_proxy(server_tls(false), false).await;
    let client = factory
        .build(trusted_client_tls(), &proxy_config(&proxy, true, false))
        .unwrap();

    assert_success(client.get(&origin.http_uri()).await.unwrap());
    assert_eq!(proxy.observations().len(), 1);
}

async fn https_via_tls_connect_proxy<F: TestClientFactory>(factory: F) {
    let origin = spawn_origin(server_tls(false)).await;
    let proxy = spawn_proxy(server_tls(false), false).await;
    let client = factory
        .build(trusted_client_tls(), &proxy_config(&proxy, true, false))
        .unwrap();

    assert_success(client.get(&origin.https_uri()).await.unwrap());
    let observations = proxy.observations();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].method, Method::CONNECT);
}

#[tokio::test]
async fn legacy_direct_http() {
    direct_http(LegacyClientFactory).await;
}

#[tokio::test]
async fn legacy_direct_https_with_trusted_ca() {
    direct_https_with_trusted_ca(LegacyClientFactory).await;
}

#[tokio::test]
async fn legacy_direct_https_rejects_untrusted_ca() {
    direct_https_rejects_untrusted_ca(LegacyClientFactory).await;
}

#[tokio::test]
async fn legacy_direct_https_honors_server_name() {
    direct_https_honors_server_name(LegacyClientFactory).await;
}

#[tokio::test]
async fn legacy_direct_https_supports_mtls() {
    direct_https_supports_mtls(LegacyClientFactory).await;
}

#[tokio::test]
async fn legacy_http_via_authenticated_proxy() {
    http_via_authenticated_proxy(LegacyClientFactory).await;
}

#[tokio::test]
async fn legacy_https_via_authenticated_connect_proxy() {
    https_via_authenticated_connect_proxy(LegacyClientFactory).await;
}

#[tokio::test]
async fn legacy_no_proxy_bypasses_proxy() {
    no_proxy_bypasses_proxy(LegacyClientFactory).await;
}

#[tokio::test]
async fn legacy_disabled_proxy_is_bypassed() {
    disabled_proxy_is_bypassed(LegacyClientFactory).await;
}

#[tokio::test]
async fn legacy_http_via_tls_proxy() {
    http_via_tls_proxy(LegacyClientFactory).await;
}

#[tokio::test]
async fn legacy_https_via_tls_connect_proxy() {
    https_via_tls_connect_proxy(LegacyClientFactory).await;
}
