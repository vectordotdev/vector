//! Coverage for [`Http2Client`] routing, TLS, ALPN, proxy credentials, and timers. The shared
//! test servers record the HTTP version and negotiated ALPN of each request.

use std::time::Duration;

use async_trait::async_trait;
use http_1::{Method, Version, header};
use hyper_util::{client::legacy::Builder, rt::TokioExecutor};
use rstest::rstest;

use super::{
    TestClient, TestResponse, assert_success, client_tls, has_proxy_authorization, native_get,
    no_tls, proxy_config, server_tls_with_alpn, spawn_origin, spawn_proxy,
};
use crate::{
    config::ProxyConfig,
    http::client_v1::{Http2Client, RequestBody},
    tls::{MaybeTlsSettings, TEST_PEM_CA_PATH, TlsConfig},
};

const H2_ALPN: &[u8] = b"h2";
const HTTP1_ALPN: &[u8] = b"http/1.1";

#[async_trait]
impl TestClient for Http2Client<RequestBody> {
    async fn get(&self, uri: &str) -> Result<TestResponse, String> {
        self.get_with_headers(uri, &[]).await
    }

    async fn get_with_headers(
        &self,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> Result<TestResponse, String> {
        native_get(uri, headers, |request| self.request(request)).await
    }
}

fn client(tls: MaybeTlsSettings, proxy: &ProxyConfig) -> Http2Client<RequestBody> {
    Http2Client::new(tls, proxy, Builder::new(TokioExecutor::new())).unwrap()
}

fn h2_server_tls() -> MaybeTlsSettings {
    server_tls_with_alpn(false, Some(vec!["h2".to_owned()]))
}

fn h2_client_tls() -> MaybeTlsSettings {
    client_tls(TlsConfig {
        ca_file: Some(TEST_PEM_CA_PATH.into()),
        alpn_protocols: Some(vec!["h2".to_owned()]),
        ..Default::default()
    })
}

#[tokio::test]
async fn h2c_uses_prior_knowledge_with_destination_authority() {
    let origin = spawn_origin(no_tls()).await;
    let client = client(no_tls(), &ProxyConfig::default());

    assert_success(client.get(&origin.http_uri()).await.unwrap());

    let observations = origin.observations();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].method, Method::GET);
    assert_eq!(observations[0].version, Version::HTTP_2);
    assert_eq!(
        observations[0].uri.to_string(),
        format!("{}/", origin.http_uri())
    );
}

#[tokio::test]
async fn h2_over_trusted_tls_offers_configured_alpn() {
    let origin = spawn_origin(h2_server_tls()).await;
    let client = client(h2_client_tls(), &ProxyConfig::default());

    assert_success(client.get(&origin.https_uri()).await.unwrap());

    let observations = origin.observations();
    assert_eq!(observations[0].version, Version::HTTP_2);
    assert_eq!(observations[0].alpn.as_deref(), Some(H2_ALPN));
}

#[tokio::test]
async fn h2_via_tls_connect_proxy_keeps_proxy_on_http1() {
    let origin = spawn_origin(h2_server_tls()).await;
    let proxy = spawn_proxy(
        server_tls_with_alpn(false, Some(vec!["h2".to_owned(), "http/1.1".to_owned()])),
        true,
    )
    .await;
    let client = client(h2_client_tls(), &proxy_config(&proxy, true, true));

    assert_success(client.get(&origin.https_uri()).await.unwrap());

    let proxy_observations = proxy.observations();
    assert_eq!(proxy_observations[0].method, Method::CONNECT);
    assert_eq!(proxy_observations[0].version, Version::HTTP_11);
    assert_eq!(proxy_observations[0].alpn.as_deref(), Some(HTTP1_ALPN));
    let origin_observations = origin.observations();
    assert_eq!(origin_observations[0].version, Version::HTTP_2);
    assert_eq!(origin_observations[0].alpn.as_deref(), Some(H2_ALPN));
}

#[rstest]
#[case::plaintext_proxy(false)]
#[case::tls_proxy(true)]
#[tokio::test]
async fn h2c_via_authenticated_forward_proxy(#[case] proxy_tls: bool) {
    let origin = spawn_origin(no_tls()).await;
    let (proxy_server_tls, client_tls_settings) = if proxy_tls {
        (h2_server_tls(), h2_client_tls())
    } else {
        (no_tls(), no_tls())
    };
    let proxy = spawn_proxy(proxy_server_tls, true).await;
    let client = client(client_tls_settings, &proxy_config(&proxy, proxy_tls, true));

    assert_success(client.get(&origin.http_uri()).await.unwrap());

    let proxy_observations = proxy.observations();
    assert_eq!(proxy_observations.len(), 1);
    assert_eq!(proxy_observations[0].method, Method::GET);
    assert_eq!(proxy_observations[0].version, Version::HTTP_2);
    assert_eq!(
        proxy_observations[0].alpn.as_deref(),
        proxy_tls.then_some(H2_ALPN)
    );
    assert_eq!(
        proxy_observations[0].uri.to_string(),
        format!("{}/", origin.http_uri())
    );
    assert!(has_proxy_authorization(&proxy_observations[0].headers));
}

#[tokio::test]
async fn no_proxy_bypass_sends_no_proxy_credentials() {
    let origin = spawn_origin(no_tls()).await;
    let proxy = spawn_proxy(no_tls(), true).await;
    let mut config = proxy_config(&proxy, false, true);
    config.no_proxy = "127.0.0.1".into();
    let client = client(no_tls(), &config);

    assert_success(client.get(&origin.http_ip_uri()).await.unwrap());

    assert!(proxy.observations().is_empty());
    let origin_observations = origin.observations();
    assert_eq!(origin_observations.len(), 1);
    assert!(
        !origin_observations[0]
            .headers
            .contains_key(header::PROXY_AUTHORIZATION)
    );
}

#[tokio::test]
async fn keepalive_client_sends_requests_without_a_caller_timer() {
    let origin = spawn_origin(no_tls()).await;
    // Hyper's HTTP/2 keepalive panics without a timer. The caller's builder sets none, so the
    // request only succeeds if `Http2Client` installs one.
    let mut builder = Builder::new(TokioExecutor::new());
    builder
        .http2_keep_alive_interval(Duration::from_secs(1))
        .http2_keep_alive_while_idle(true);
    let client: Http2Client<RequestBody> =
        Http2Client::new(no_tls(), &ProxyConfig::default(), builder).unwrap();

    assert_success(client.get(&origin.http_uri()).await.unwrap());
}
