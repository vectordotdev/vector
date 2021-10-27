use crate::{
    buffers::Acker,
    config::ProxyConfig,
    http::HttpClient,
    sinks::util::buffer::Compression,
    sinks::util::http::BatchedHttpSink,
    sinks::util::service::TowerRequestConfig,
    sinks::util::{BatchConfig, BatchSettings, Buffer},
    sinks::UriParseError,
    sinks::{splunk_hec::common::build_healthcheck, util::http::HttpSink},
    sinks::{Healthcheck, VectorSink},
    tls::{TlsOptions, TlsSettings},
};
use futures::{FutureExt, SinkExt};
use http::Uri;
use snafu::{ResultExt, Snafu};
use std::convert::TryFrom;

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
        .bytes(1_000_000)
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

    let healthcheck = build_healthcheck(endpoint.to_string(), token.to_string(), client).boxed();

    Ok((VectorSink::Sink(Box::new(sink)), healthcheck))
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
    use crate::sinks::splunk_hec::common::build_uri;
    use http::Request;
    use std::path::PathBuf;
    use wiremock::matchers::{body_string, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
