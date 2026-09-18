use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use http::{HeaderMap, Method, Request, Response, StatusCode, Uri, header};
use hyper::{Body, body::HttpBody as _, client::conn, server::conn::http1, service::service_fn};
use rstest::rstest;
use tokio::{io::copy_bidirectional, net::TcpStream, task::JoinHandle, time::timeout};

use super::{
    HttpClient as LegacyHttpClient,
    client_v1::{HttpClient, empty_body},
};
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
        let mut request = Request::get(uri)
            .body(Body::empty())
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
        let mut request = http_1::Request::get(uri)
            .body(empty_body())
            .map_err(|error| error.to_string())?;
        for &(name, value) in headers {
            request.headers_mut().insert(
                http_1::header::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|error| error.to_string())?,
                value
                    .parse()
                    .map_err(|error: http_1::header::InvalidHeaderValue| error.to_string())?,
            );
        }
        let response = timeout(REQUEST_TIMEOUT, self.send(request))
            .await
            .map_err(|_| "request timed out".to_owned())?
            .map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let body = timeout(
            REQUEST_TIMEOUT,
            http_body_util::BodyExt::collect(response.into_body()),
        )
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
    spawn_proxy_with_connect_status(tls, require_authentication, StatusCode::OK).await
}

async fn spawn_proxy_with_connect_status(
    tls: MaybeTlsSettings,
    require_authentication: bool,
    connect_status: StatusCode,
) -> TestServer {
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
                            proxy_request(
                                request,
                                observations,
                                require_authentication,
                                connect_status,
                            )
                            .await,
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
    connect_status: StatusCode,
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
        return Response::builder()
            .status(connect_status)
            .body(Body::empty())
            .unwrap();
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
