//! OAuth 2.0 client-credentials tests for [`HttpClient`].

use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use bytes::Bytes;
use headers::{
    authorization::Basic,
    {Authorization, HeaderMapExt},
};
use http::{HeaderMap, Request, Response, StatusCode, header};
use http_body_util::{BodyExt as _, Full};
use hyper::{body::Incoming, service::service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto,
};
use vector_lib::sensitive_string::SensitiveString;

use super::{ServerResponse, assert_success, no_tls, server_tls, spawn_origin};
use crate::{
    config::ProxyConfig,
    http::{HttpClient, OAuth2Config, OAuth2Extension},
    tls::{
        MaybeTlsSettings, TEST_PEM_CA_PATH, TEST_PEM_CLIENT_CRT_PATH, TEST_PEM_CLIENT_KEY_PATH,
        TlsConfig,
    },
};

const ACCESS_TOKEN: &str = "test-access-token";

/// A token endpoint request that the server received.
#[derive(Clone, Debug)]
struct TokenRequest {
    headers: HeaderMap,
    body: Vec<u8>,
}

impl TokenRequest {
    fn form(&self) -> std::collections::HashMap<String, String> {
        url::form_urlencoded::parse(&self.body)
            .into_owned()
            .collect()
    }
}

/// A small server that records each request and answers with `respond()`.
struct TokenServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<TokenRequest>>>,
    task: tokio::task::JoinHandle<()>,
}

impl TokenServer {
    fn http_uri(&self) -> String {
        format!("http://localhost:{}", self.addr.port())
    }

    fn https_uri(&self) -> String {
        format!("https://localhost:{}", self.addr.port())
    }

    fn token_endpoint(&self, tls: bool) -> String {
        if tls {
            format!("{}/oauth/token", self.https_uri())
        } else {
            format!("{}/oauth/token", self.http_uri())
        }
    }

    fn requests(&self) -> Vec<TokenRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for TokenServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn spawn_token_server<R, F>(tls: MaybeTlsSettings, respond: R) -> TokenServer
where
    R: Fn() -> F + Clone + Send + Sync + 'static,
    F: std::future::Future<Output = ServerResponse> + Send + 'static,
{
    let addr = "127.0.0.1:0".parse().unwrap();
    let mut listener = tls.bind(&addr).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let server_requests = Arc::clone(&requests);

    let task = tokio::spawn(async move {
        loop {
            let Ok(stream) = listener.accept().await else {
                continue;
            };
            let requests = Arc::clone(&server_requests);
            let respond = respond.clone();
            tokio::spawn(async move {
                let mut stream = stream;
                if stream.handshake().await.is_err() {
                    return;
                }
                let service = service_fn(move |request: Request<Incoming>| {
                    let requests = Arc::clone(&requests);
                    let respond = respond.clone();
                    async move {
                        let (parts, body) = request.into_parts();
                        let body = body.collect().await.map(|body| body.to_bytes().to_vec());
                        requests.lock().unwrap().push(TokenRequest {
                            headers: parts.headers,
                            body: body.unwrap_or_default(),
                        });
                        Ok::<_, Infallible>(respond().await)
                    }
                });
                if let Err(error) = auto::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(stream), service)
                    .await
                {
                    tracing::debug!(message = "Test token server connection closed.", %error);
                }
            });
        }
    });

    TokenServer {
        addr,
        requests,
        task,
    }
}

fn token_response(expires_in: u64) -> ServerResponse {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(format!(
            r#"{{"access_token":"{ACCESS_TOKEN}","expires_in":{expires_in}}}"#
        ))))
        .unwrap()
}

fn error_response(status: StatusCode) -> ServerResponse {
    Response::builder()
        .status(status)
        .body(Full::new(Bytes::from_static(b"rejected")))
        .unwrap()
}

fn oauth2_config(token_endpoint: String, expiry_buffer_secs: u32) -> OAuth2Config {
    OAuth2Config {
        client_id: "my-client".to_owned(),
        client_secret: Some(SensitiveString::from("my-secret".to_owned())),
        token_endpoint,
        scope: None,
        audience: None,
        resource: None,
        expiry_buffer_secs,
        timeout_secs: 10,
        tls: None,
    }
}

fn client_with_oauth2(config: OAuth2Config, fallback_tls: Option<&TlsConfig>) -> HttpClient {
    let extension = OAuth2Extension::new(config, fallback_tls, &ProxyConfig::default())
        .expect("valid settings");
    HttpClient::new(no_tls(), &ProxyConfig::default())
        .expect("valid client")
        .with_oauth2(extension)
}

async fn get(client: &HttpClient, uri: &str) -> Result<(), vector_http::HttpError> {
    client
        .send(Request::get(uri).body(vector_http::empty_body()).unwrap())
        .await
        .map(drop)
}

fn bearer_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::to_owned)
}

#[tokio::test]
async fn oauth2_token_is_fetched_once_and_reused() {
    let token_server = spawn_token_server(no_tls(), || async { token_response(3600) }).await;
    let origin = spawn_origin(no_tls()).await;
    let client = client_with_oauth2(oauth2_config(token_server.token_endpoint(false), 60), None);

    get(&client, &origin.http_uri()).await.unwrap();
    get(&client, &origin.http_uri()).await.unwrap();

    assert_eq!(
        token_server.requests().len(),
        1,
        "the token must be fetched once and reused"
    );
    let observations = origin.observations();
    assert_eq!(observations.len(), 2);
    for observation in observations {
        assert_eq!(
            bearer_of(&observation.headers),
            Some(ACCESS_TOKEN.to_owned())
        );
    }
}

#[tokio::test]
async fn oauth2_concurrent_sends_share_one_token_fetch() {
    let token_server = spawn_token_server(no_tls(), || async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        token_response(3600)
    })
    .await;
    let origin = spawn_origin(no_tls()).await;
    let origin_uri = origin.http_uri();
    let client = client_with_oauth2(oauth2_config(token_server.token_endpoint(false), 60), None);

    let sends = (0..8).map(|_| get(&client, &origin_uri));
    let results = futures::future::join_all(sends).await;

    assert!(results.iter().all(Result::is_ok));
    assert_eq!(
        token_server.requests().len(),
        1,
        "concurrent requests must not race for a token"
    );
}

#[tokio::test]
async fn oauth2_token_expiring_at_once_is_refetched() {
    let token_server = spawn_token_server(no_tls(), || async { token_response(20) }).await;
    let origin = spawn_origin(no_tls()).await;
    // The expiry buffer is equal to or larger than the lifetime. Thus each access token
    // expires immediately.
    let client = client_with_oauth2(oauth2_config(token_server.token_endpoint(false), 60), None);

    get(&client, &origin.http_uri()).await.unwrap();
    get(&client, &origin.http_uri()).await.unwrap();

    assert_eq!(token_server.requests().len(), 2);
}

#[tokio::test]
async fn oauth2_token_request_uses_basic_authentication() {
    let token_server = spawn_token_server(no_tls(), || async { token_response(3600) }).await;
    let origin = spawn_origin(no_tls()).await;
    let mut config = oauth2_config(token_server.token_endpoint(false), 60);
    config.client_id = "client id&scope=all".to_owned();
    config.client_secret = Some(SensitiveString::from("secret&x=y".to_owned()));
    let client = client_with_oauth2(config, None);

    get(&client, &origin.http_uri()).await.unwrap();

    let requests = token_server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].headers.get(header::CONTENT_TYPE).unwrap(),
        "application/x-www-form-urlencoded"
    );
    let form = requests[0].form();
    assert_eq!(form["grant_type"], "client_credentials");
    assert!(
        !form.contains_key("client_id") && !form.contains_key("client_secret"),
        "RFC 6749 section 2.3.1 allows one authentication method per request"
    );

    // The credentials travel percent-encoded inside the Basic credentials.
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        requests[0]
            .headers
            .get(header::AUTHORIZATION)
            .unwrap()
            .clone(),
    );
    let authorization = headers
        .typed_get::<Authorization<Basic>>()
        .expect("the token request must use basic authentication");
    assert_eq!(authorization.username(), "client+id%26scope%3Dall");
    assert_eq!(authorization.password(), "secret%26x%3Dy");
}

#[tokio::test]
async fn oauth2_token_endpoint_supports_mutual_tls() {
    let token_server =
        spawn_token_server(server_tls(true), || async { token_response(3600) }).await;
    let origin = spawn_origin(no_tls()).await;
    let mut config = oauth2_config(token_server.token_endpoint(true), 60);
    // RFC 8705: no client secret. The client authenticates with a certificate.
    config.client_secret = None;
    config.tls = Some(TlsConfig {
        ca_file: Some(TEST_PEM_CA_PATH.into()),
        crt_file: Some(TEST_PEM_CLIENT_CRT_PATH.into()),
        key_file: Some(TEST_PEM_CLIENT_KEY_PATH.into()),
        ..Default::default()
    });
    let client = client_with_oauth2(config, None);

    get(&client, &origin.http_uri()).await.unwrap();

    let requests = token_server.requests();
    assert_eq!(requests.len(), 1);
    assert!(
        !requests[0].form().contains_key("client_secret"),
        "the client secret must be absent from a mutual-TLS token request"
    );
    assert!(
        !requests[0].headers.contains_key(header::AUTHORIZATION),
        "mutual TLS must not send basic credentials"
    );
}

#[tokio::test]
async fn oauth2_falls_back_to_the_sink_tls_settings() {
    let token_server =
        spawn_token_server(server_tls(true), || async { token_response(3600) }).await;
    let origin = spawn_origin(no_tls()).await;
    let mut config = oauth2_config(token_server.token_endpoint(true), 60);
    config.client_secret = None;
    // `config.tls` is unset. Vector must use the fallback.
    let fallback = TlsConfig {
        ca_file: Some(TEST_PEM_CA_PATH.into()),
        crt_file: Some(TEST_PEM_CLIENT_CRT_PATH.into()),
        key_file: Some(TEST_PEM_CLIENT_KEY_PATH.into()),
        ..Default::default()
    };
    let client = client_with_oauth2(config, Some(&fallback));

    get(&client, &origin.http_uri()).await.unwrap();
    assert_eq!(token_server.requests().len(), 1);
}

#[tokio::test]
async fn oauth2_server_error_is_retriable() {
    let token_server = spawn_token_server(no_tls(), || async {
        error_response(StatusCode::INTERNAL_SERVER_ERROR)
    })
    .await;
    let origin = spawn_origin(no_tls()).await;
    let client = client_with_oauth2(oauth2_config(token_server.token_endpoint(false), 60), None);

    let error = get(&client, &origin.http_uri())
        .await
        .expect_err("a 500 token response must fail the request");
    assert!(error.is_retriable());
}

#[tokio::test]
async fn oauth2_unsupported_token_type_is_not_retriable() {
    let token_server = spawn_token_server(no_tls(), || async {
        Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from_static(
                br#"{"access_token":"t","token_type":"MAC","expires_in":3600}"#,
            )))
            .unwrap()
    })
    .await;
    let origin = spawn_origin(no_tls()).await;
    let client = client_with_oauth2(oauth2_config(token_server.token_endpoint(false), 60), None);

    let error = get(&client, &origin.http_uri())
        .await
        .expect_err("a token that is not a bearer token must fail the request");
    assert!(
        !error.is_retriable(),
        "a retry cannot fix an unsupported token type"
    );
}

#[tokio::test]
async fn oauth2_too_many_requests_is_retriable() {
    let token_server = spawn_token_server(no_tls(), || async {
        error_response(StatusCode::TOO_MANY_REQUESTS)
    })
    .await;
    let origin = spawn_origin(no_tls()).await;
    let client = client_with_oauth2(oauth2_config(token_server.token_endpoint(false), 60), None);

    let error = get(&client, &origin.http_uri())
        .await
        .expect_err("a 429 token response must fail the request");
    assert!(error.is_retriable());
}

#[tokio::test]
async fn oauth2_bad_request_is_not_retriable() {
    let token_server = spawn_token_server(no_tls(), || async {
        error_response(StatusCode::BAD_REQUEST)
    })
    .await;
    let origin = spawn_origin(no_tls()).await;
    let client = client_with_oauth2(oauth2_config(token_server.token_endpoint(false), 60), None);

    let error = get(&client, &origin.http_uri())
        .await
        .expect_err("a 400 token response must fail the request");
    assert!(!error.is_retriable());
}

#[tokio::test]
async fn oauth2_token_request_carries_the_proxy_credentials() {
    let proxy = super::spawn_proxy(no_tls(), true).await;
    let token_server = spawn_token_server(no_tls(), || async { token_response(3600) }).await;
    let origin = spawn_origin(no_tls()).await;
    let proxy_config = super::proxy_config(&proxy, false, true);

    let extension = OAuth2Extension::new(
        oauth2_config(token_server.token_endpoint(false), 60),
        None,
        &proxy_config,
    )
    .expect("valid settings");
    let client = HttpClient::new(no_tls(), &proxy_config)
        .expect("valid client")
        .with_oauth2(extension);

    get(&client, &origin.http_uri()).await.unwrap();
    assert_eq!(
        token_server.requests().len(),
        1,
        "the proxy must authenticate and forward the token request"
    );
}

#[tokio::test]
async fn oauth2_token_request_times_out() {
    let token_server = spawn_token_server(no_tls(), || async {
        tokio::time::sleep(Duration::from_secs(5)).await;
        token_response(3600)
    })
    .await;
    let origin = spawn_origin(no_tls()).await;
    let mut config = oauth2_config(token_server.token_endpoint(false), 60);
    config.timeout_secs = 1;
    let client = client_with_oauth2(config, None);

    let error = get(&client, &origin.http_uri())
        .await
        .expect_err("a token request that exceeds the timeout must fail");
    assert!(error.is_retriable(), "a timeout must be retriable");
}

#[tokio::test]
async fn oauth2_unauthorized_response_discards_the_cached_token() {
    let fetched = Arc::new(std::sync::atomic::AtomicUsize::new(1));
    let token_server = spawn_token_server(no_tls(), move || {
        let number = fetched.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async move {
            Response::builder()
                .header(header::CONTENT_TYPE, "application/json")
                .body(Full::new(Bytes::from(format!(
                    r#"{{"access_token":"token-{number}","expires_in":3600}}"#
                ))))
                .unwrap()
        }
    })
    .await;
    // The origin accepts only the second access token. It rejects the first one with a 401 that
    // stands for a token the authorization server has already revoked.
    let origin = super::spawn_server(no_tls(), |request: Request<Incoming>| async move {
        if bearer_of(request.headers()).as_deref() == Some("token-2") {
            Response::new(Full::new(Bytes::from_static(b"ok")))
        } else {
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Full::new(Bytes::from_static(b"expired")))
                .unwrap()
        }
    })
    .await;
    let client = client_with_oauth2(oauth2_config(token_server.token_endpoint(false), 60), None);
    let origin_uri = origin.http_uri();

    get(&client, &origin_uri).await.unwrap();
    get(&client, &origin_uri).await.unwrap();

    assert_eq!(
        token_server.requests().len(),
        2,
        "a 401 response must discard the cached access token"
    );
    let observations = origin.observations();
    assert_eq!(
        bearer_of(&observations[0].headers),
        Some("token-1".to_owned())
    );
    assert_eq!(
        bearer_of(&observations[1].headers),
        Some("token-2".to_owned())
    );
}

#[tokio::test]
async fn oauth2_does_not_affect_a_client_without_it() {
    let origin = spawn_origin(no_tls()).await;
    let client = HttpClient::new(no_tls(), &ProxyConfig::default()).expect("valid client");

    let response = client
        .send(
            Request::get(origin.http_uri())
                .body(vector_http::empty_body())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_success(super::TestResponse {
        status: response.status().as_u16(),
        body: response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    });
    assert!(bearer_of(&origin.observations()[0].headers).is_none());
}
