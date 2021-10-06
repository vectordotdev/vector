use crate::{
    buffers::Acker,
    config::ProxyConfig,
    http::HttpClient,
    sinks,
    sinks::util::buffer::Compression,
    sinks::util::http::BatchedHttpSink,
    sinks::util::http::HttpSink,
    sinks::util::service::TowerRequestConfig,
    sinks::util::{BatchConfig, BatchSettings, Buffer},
    sinks::UriParseError,
    sinks::{Healthcheck, VectorSink},
    tls::{TlsOptions, TlsSettings},
};
use futures::{FutureExt, SinkExt};
use http::{Request, StatusCode, Uri};
use hyper::Body;
use snafu::{ResultExt, Snafu};
use std::convert::TryFrom;

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Invalid HEC token"))]
    InvalidToken,
    #[snafu(display("Queues are full"))]
    QueuesFull,
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Host must include a scheme (https:// or http://)"))]
    UriMissingScheme,
}

pub fn build_sink<T>(
    sink: T,
    request_config: &TowerRequestConfig,
    tls_options: &Option<TlsOptions>,
    proxy_config: &ProxyConfig,
    batch_config: BatchConfig,
    compression: Compression,
    acker: Acker,
    endpoint: &str,
    token: &str,
) -> crate::Result<(VectorSink, Healthcheck)>
where
    T: HttpSink<Input = Vec<u8>, Output = Vec<u8>>,
{
    validate_host(endpoint)?;

    let batch_settings = BatchSettings::default()
        .bytes(bytesize::mib(1u64))
        .timeout(1)
        .parse_config(batch_config)?;
    let request_settings = request_config.unwrap_with(&TowerRequestConfig::default());
    let tls_settings = TlsSettings::from_options(tls_options)?;
    let client = HttpClient::new(tls_settings, proxy_config)?;

    let sink = BatchedHttpSink::new(
        sink,
        Buffer::new(batch_settings.size, compression),
        request_settings,
        batch_settings.timeout,
        client.clone(),
        acker,
    )
    .sink_map_err(|error| error!(message = "Fatal splunk_hec sink error.", %error));

    let healthcheck = healthcheck(endpoint.to_string(), token.to_string(), client).boxed();

    Ok((VectorSink::Sink(Box::new(sink)), healthcheck))
}

pub async fn build_request(
    endpoint: &str,
    token: &str,
    compression: Compression,
    events: Vec<u8>,
) -> crate::Result<Request<Vec<u8>>> {
    let uri = build_uri(endpoint, "/services/collector/event").context(UriParseError)?;

    let mut builder = Request::post(uri)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Splunk {}", token));

    if let Some(ce) = compression.content_encoding() {
        builder = builder.header("Content-Encoding", ce);
    }

    builder.body(events).map_err(Into::into)
}

async fn healthcheck(endpoint: String, token: String, client: HttpClient) -> crate::Result<()> {
    let uri =
        build_uri(endpoint.as_str(), "/services/collector/health/1.0").context(UriParseError)?;

    let request = Request::get(uri)
        .header("Authorization", format!("Splunk {}", token))
        .body(Body::empty())
        .unwrap();

    let response = client.send(request).await?;
    match response.status() {
        StatusCode::OK => Ok(()),
        StatusCode::BAD_REQUEST => Err(HealthcheckError::InvalidToken.into()),
        StatusCode::SERVICE_UNAVAILABLE => Err(HealthcheckError::QueuesFull.into()),
        other => Err(sinks::HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}

fn build_uri(host: &str, path: &str) -> Result<Uri, http::uri::InvalidUri> {
    format!("{}{}", host.trim_end_matches('/'), path).parse::<Uri>()
}

fn validate_host(host: &str) -> crate::Result<()> {
    let uri = Uri::try_from(host).context(UriParseError)?;

    match uri.scheme() {
        Some(_) => Ok(()),
        None => Err(Box::new(BuildError::UriMissingScheme)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use http::HeaderValue;
    use std::path::PathBuf;
    use wiremock::matchers::{body_string, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_build_request_compression_none_returns_expected_request() {
        let endpoint = "http://localhost:8888";
        let token = "token";
        let compression = Compression::None;
        let events = "events".as_bytes().to_vec();

        let request = build_request(endpoint, token, compression, events.clone())
            .await
            .unwrap();

        assert_eq!(
            request.uri(),
            &Uri::from_static("http://localhost:8888/services/collector/event")
        );

        assert_eq!(
            request.headers().get("Content-Type"),
            Some(&HeaderValue::from_static("application/json"))
        );

        assert_eq!(
            request.headers().get("Authorization"),
            Some(&HeaderValue::from_static("Splunk token"))
        );

        assert_eq!(request.headers().get("Content-Encoding"), None);

        assert_eq!(request.body(), &events)
    }

    #[tokio::test]
    async fn test_build_request_compression_gzip_returns_expected_request() {
        let endpoint = "http://localhost:8888";
        let token = "token";
        let compression = Compression::gzip_default();
        let events = "events".as_bytes().to_vec();

        let request = build_request(endpoint, token, compression, events.clone())
            .await
            .unwrap();

        assert_eq!(
            request.uri(),
            &Uri::from_static("http://localhost:8888/services/collector/event")
        );

        assert_eq!(
            request.headers().get("Content-Type"),
            Some(&HeaderValue::from_static("application/json"))
        );

        assert_eq!(
            request.headers().get("Authorization"),
            Some(&HeaderValue::from_static("Splunk token"))
        );

        assert_eq!(
            request.headers().get("Content-Encoding"),
            Some(&HeaderValue::from_static("gzip"))
        );

        assert_eq!(request.body(), &events)
    }

    #[tokio::test]
    async fn test_build_request_uri_invalid_uri_returns_error() {
        let endpoint = "invalid";
        let token = "token";
        let compression = Compression::gzip_default();
        let events = "events".as_bytes().to_vec();

        let err = build_request(endpoint, token, compression, events.clone())
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "URI parse error: invalid format")
    }

    #[tokio::test]
    async fn test_build_sink_sink_calls_expected_endpoint() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/stub-path"))
            .and(body_string("test encoded event"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let stub_sink = StubSink {
            endpoint: mock_server.uri(),
        };

        let (sink, _) = build_sink(
            stub_sink,
            &TowerRequestConfig::default(),
            &None,
            &ProxyConfig::default(),
            BatchConfig::default(),
            Compression::None,
            Acker::Null,
            &mock_server.uri(),
            "token",
        )
        .unwrap();

        let mut sink = sink.into_sink();

        sink.send(Event::from("test event")).await.unwrap();
        sink.flush().await.unwrap();
    }

    #[tokio::test]
    async fn test_build_sink_healthcheck_200_response_returns_ok() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/services/collector/health/1.0"))
            .and(header("Authorization", "Splunk token"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let (_, healthcheck) = build_sink(
            StubSink::default(),
            &TowerRequestConfig::default(),
            &None,
            &ProxyConfig::default(),
            BatchConfig::default(),
            Compression::None,
            Acker::Null,
            &mock_server.uri(),
            "token",
        )
        .unwrap();

        assert!(healthcheck.await.is_ok())
    }

    #[tokio::test]
    async fn test_build_sink_healthcheck_400_response_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/services/collector/health/1.0"))
            .and(header("Authorization", "Splunk token"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&mock_server)
            .await;

        let (_, healthcheck) = build_sink(
            StubSink::default(),
            &TowerRequestConfig::default(),
            &None,
            &ProxyConfig::default(),
            BatchConfig::default(),
            Compression::None,
            Acker::Null,
            &mock_server.uri(),
            "token",
        )
        .unwrap();

        assert_eq!(
            &healthcheck.await.unwrap_err().to_string(),
            "Invalid HEC token"
        );
    }

    #[tokio::test]
    async fn test_build_sink_healthcheck_503_response_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/services/collector/health/1.0"))
            .and(header("Authorization", "Splunk token"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock_server)
            .await;

        let (_, healthcheck) = build_sink(
            StubSink::default(),
            &TowerRequestConfig::default(),
            &None,
            &ProxyConfig::default(),
            BatchConfig::default(),
            Compression::None,
            Acker::Null,
            &mock_server.uri(),
            "token",
        )
        .unwrap();

        assert_eq!(
            &healthcheck.await.unwrap_err().to_string(),
            "Queues are full"
        );
    }

    #[tokio::test]
    async fn test_build_sink_healthcheck_500_response_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/services/collector/health/1.0"))
            .and(header("Authorization", "Splunk token"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let (_, healthcheck) = build_sink(
            StubSink::default(),
            &TowerRequestConfig::default(),
            &None,
            &ProxyConfig::default(),
            BatchConfig::default(),
            Compression::None,
            Acker::Null,
            &mock_server.uri(),
            "token",
        )
        .unwrap();

        assert_eq!(
            &healthcheck.await.unwrap_err().to_string(),
            "Unexpected status: 500 Internal Server Error"
        );
    }

    #[tokio::test]
    async fn test_build_sink_healthcheck_service_not_listening_returns_error() {
        let non_listening_uri = "http://localhost:36448/";

        let (_, healthcheck) = build_sink(
            StubSink::default(),
            &TowerRequestConfig::default(),
            &None,
            &ProxyConfig::default(),
            BatchConfig::default(),
            Compression::None,
            Acker::Null,
            non_listening_uri,
            "token",
        )
        .unwrap();

        assert!(&healthcheck.await.unwrap_err().to_string().starts_with(
            "Failed to make HTTP(S) request: error trying to connect: tcp connect error:"
        ));
    }

    #[tokio::test]
    async fn test_build_sink_endpoint_uri_invalid_returns_error() {
        let invalid_uri = "";

        let err = build_sink(
            StubSink::default(),
            &TowerRequestConfig::default(),
            &None,
            &ProxyConfig::default(),
            BatchConfig::default(),
            Compression::None,
            Acker::Null,
            invalid_uri,
            "token",
        )
        .err()
        .unwrap();

        assert_eq!(err.to_string(), "URI parse error: empty string")
    }

    #[tokio::test]
    async fn test_build_sink_endpoint_uri_no_protocol_returns_error() {
        let invalid_uri = "localhost:36448";

        let err = build_sink(
            StubSink::default(),
            &TowerRequestConfig::default(),
            &None,
            &ProxyConfig::default(),
            BatchConfig::default(),
            Compression::None,
            Acker::Null,
            invalid_uri,
            "token",
        )
        .err()
        .unwrap();

        assert_eq!(
            err.to_string(),
            "Host must include a scheme (https:// or http://)"
        )
    }

    #[tokio::test]
    async fn test_build_sink_invalid_tls_options_returns_error() {
        let invalid_tls_options = Some(TlsOptions {
            verify_certificate: Some(true),
            verify_hostname: Some(true),
            ca_file: None,
            crt_file: None,
            key_file: Some(PathBuf::from("test_value")),
            key_pass: None,
        });

        let err = build_sink(
            StubSink::default(),
            &TowerRequestConfig::default(),
            &invalid_tls_options,
            &ProxyConfig::default(),
            BatchConfig::default(),
            Compression::None,
            Acker::Null,
            "http://localhost:36448",
            "token",
        )
        .err()
        .unwrap();

        assert_eq!(
            err.to_string(),
            "Must specify both TLS key_file and crt_file"
        )
    }

    #[tokio::test]
    async fn test_build_sink_invalid_tls_file_paths_returns_error() {
        let invalid_tls_options = Some(TlsOptions {
            verify_certificate: Some(true),
            verify_hostname: Some(true),
            ca_file: None,
            crt_file: Some(PathBuf::from("invalid_path")),
            key_file: Some(PathBuf::from("invalid_path")),
            key_pass: None,
        });

        let err = build_sink(
            StubSink::default(),
            &TowerRequestConfig::default(),
            &invalid_tls_options,
            &ProxyConfig::default(),
            BatchConfig::default(),
            Compression::None,
            Acker::Null,
            "http://localhost:36448",
            "token",
        )
        .err()
        .unwrap();

        assert!(err
            .to_string()
            .starts_with("Could not open certificate file \"invalid_path\":"));
    }

    #[tokio::test]
    async fn test_build_sink_invalid_proxy_config_returns_error() {
        let invalid_proxy_config = ProxyConfig {
            enabled: true,
            http: Some(String::from("")),
            https: None,
            no_proxy: Default::default(),
        };

        let err = build_sink(
            StubSink::default(),
            &TowerRequestConfig::default(),
            &None,
            &invalid_proxy_config,
            BatchConfig::default(),
            Compression::None,
            Acker::Null,
            "http://localhost:36448",
            "token",
        )
        .err()
        .unwrap();

        assert_eq!(
            err.to_string(),
            "Failed to build Proxy connector: empty string"
        )
    }

    #[derive(Default)]
    struct StubSink {
        endpoint: String,
    }

    #[async_trait::async_trait]
    impl HttpSink for StubSink {
        type Input = Vec<u8>;
        type Output = Vec<u8>;

        fn encode_event(&self, _: Event) -> Option<Self::Input> {
            Some(String::from("test encoded event").into_bytes())
        }

        async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
            let uri = build_uri(&self.endpoint, "/stub-path")?;

            Request::post(uri).body(events).map_err(Into::into)
        }
    }
}

#[cfg(all(test, feature = "splunk-integration-tests"))]
mod integration_tests {
    use super::*;
    use crate::{assert_downcast_matches, tls::TlsSettings};
    use integration_test_helpers::get_token;
    use std::net::SocketAddr;
    use warp::Filter;

    #[tokio::test]
    async fn splunk_healthcheck() {
        let create_healthcheck = |endpoint: String, token: String| {
            let tls_settings = TlsSettings::from_options(&None).unwrap();
            let proxy = ProxyConfig::default();
            let client = HttpClient::new(tls_settings, &proxy).unwrap();
            super::healthcheck(endpoint, token, client)
        };

        // OK
        {
            let healthcheck =
                create_healthcheck("http://localhost:8088/".to_string(), get_token().await);
            healthcheck.await.unwrap();
        }

        // Server not listening at address
        {
            let healthcheck =
                create_healthcheck("http://localhost:1111".to_string(), get_token().await);
            healthcheck.await.unwrap_err();
        }

        // Unhealthy server
        {
            let healthcheck =
                create_healthcheck("http://localhost:5503".to_string(), get_token().await);

            let unhealthy = warp::any()
                .map(|| warp::reply::with_status("i'm sad", StatusCode::SERVICE_UNAVAILABLE));
            let server = warp::serve(unhealthy).bind("0.0.0.0:5503".parse::<SocketAddr>().unwrap());
            tokio::spawn(server);

            assert_downcast_matches!(
                healthcheck.await.unwrap_err(),
                HealthcheckError,
                HealthcheckError::QueuesFull
            );
        }
    }
}

#[cfg(all(test, feature = "splunk-integration-tests"))]
pub mod integration_test_helpers {
    use crate::test_util::retry_until;
    use serde_json::Value as JsonValue;
    use tokio::time::Duration;

    const USERNAME: &str = "admin";
    const PASSWORD: &str = "password";

    pub async fn get_token() -> String {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let res = retry_until(
            || {
                client
                    .get("https://localhost:8089/services/data/inputs/http?output_mode=json")
                    .basic_auth(USERNAME, Some(PASSWORD))
                    .send()
            },
            Duration::from_millis(500),
            Duration::from_secs(30),
        )
        .await;

        let json: JsonValue = res.json().await.unwrap();
        let entries = json["entry"].as_array().unwrap().clone();

        if entries.is_empty() {
            panic!("You don't have any HTTP Event Collector inputs set up in Splunk");
        }

        entries[0]["content"]["token"].as_str().unwrap().to_owned()
    }
}
