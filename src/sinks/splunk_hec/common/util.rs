use std::{borrow::Cow, sync::Arc};

use bytes::Bytes;
use futures_util::future::BoxFuture;
use http::{Request, StatusCode, Uri};
use hyper::Body;
use snafu::{ResultExt, Snafu};
use vector_lib::lookup::lookup_v2::{OptionalTargetPath, OptionalValuePath};
use vector_lib::{config::proxy::ProxyConfig, event::EventRef};

use super::{
    request::HecRequest,
    service::{HttpRequestBuilder, MetadataFields},
    EndpointTarget,
};
use crate::{
    http::HttpClient,
    internal_events::TemplateRenderingError,
    sinks::{
        self,
        util::{http::HttpBatchService, SinkBatchSettings},
        UriParseSnafu,
    },
    template::Template,
    tls::{TlsConfig, TlsSettings},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct SplunkHecDefaultBatchSettings;

impl SinkBatchSettings for SplunkHecDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(1_000_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Debug, Snafu)]
pub enum HealthcheckError {
    #[snafu(display("Invalid HEC token"))]
    InvalidToken,
    #[snafu(display("Queues are full"))]
    QueuesFull,
}

pub fn create_client(
    tls: &Option<TlsConfig>,
    proxy_config: &ProxyConfig,
) -> crate::Result<HttpClient> {
    let tls_settings = TlsSettings::from_options(tls)?;
    Ok(HttpClient::new(tls_settings, proxy_config)?)
}

// TODO: `HttpBatchService` has been deprecated for direct use in sinks.
//       This sink should undergo a refactor to utilize the `HttpService`
//       instead, which extracts much of the boilerplate code for `Service`.
pub fn build_http_batch_service(
    client: HttpClient,
    http_request_builder: Arc<HttpRequestBuilder>,
    endpoint_target: EndpointTarget,
    auto_extract_timestamp: bool,
) -> HttpBatchService<BoxFuture<'static, Result<Request<Bytes>, crate::Error>>, HecRequest> {
    HttpBatchService::new(client, move |req: HecRequest| {
        let request_builder = Arc::clone(&http_request_builder);
        let future: BoxFuture<'static, Result<http::Request<Bytes>, crate::Error>> =
            Box::pin(async move {
                request_builder.build_request(
                    req.body,
                    match endpoint_target {
                        EndpointTarget::Event => "/services/collector/event",
                        EndpointTarget::Raw => "/services/collector/raw",
                    },
                    req.passthrough_token,
                    MetadataFields {
                        source: req.source,
                        sourcetype: req.sourcetype,
                        index: req.index,
                        host: req.host,
                    },
                    auto_extract_timestamp,
                )
            });
        future
    })
}

pub async fn build_healthcheck(
    endpoint: String,
    token: String,
    client: HttpClient,
) -> crate::Result<()> {
    let uri = build_uri(endpoint.as_str(), "/services/collector/health/1.0", None)
        .context(UriParseSnafu)?;

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

pub fn build_uri(
    host: &str,
    path: &str,
    query: impl IntoIterator<Item = (&'static str, String)>,
) -> Result<Uri, http::uri::InvalidUri> {
    let mut uri = format!("{}{}", host.trim_end_matches('/'), path);

    let mut first = true;

    for (key, value) in query.into_iter() {
        if first {
            uri.push('?');
            first = false;
        } else {
            uri.push('&');
        }
        uri.push_str(&Cow::<str>::from(percent_encoding::utf8_percent_encode(
            key,
            percent_encoding::NON_ALPHANUMERIC,
        )));
        uri.push('=');
        uri.push_str(&Cow::<str>::from(percent_encoding::utf8_percent_encode(
            &value,
            percent_encoding::NON_ALPHANUMERIC,
        )));
    }

    uri.parse::<Uri>()
}

pub fn config_host_key() -> OptionalValuePath {
    OptionalValuePath {
        path: crate::config::log_schema().host_key().cloned(),
    }
}

pub fn config_timestamp_key_target_path() -> OptionalTargetPath {
    OptionalTargetPath {
        path: crate::config::log_schema()
            .timestamp_key_target_path()
            .cloned(),
    }
}

pub fn render_template_string<'a>(
    template: &Template,
    event: impl Into<EventRef<'a>>,
    field_name: &str,
) -> Option<String> {
    template
        .render_string(event)
        .map_err(|error| {
            emit!(TemplateRenderingError {
                error,
                field: Some(field_name),
                drop_event: false
            });
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http::{HeaderValue, Uri};
    use vector_lib::config::proxy::ProxyConfig;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::sinks::{
        splunk_hec::common::{
            build_healthcheck, build_uri, create_client,
            service::{HttpRequestBuilder, MetadataFields},
            EndpointTarget, HOST_FIELD, SOURCE_FIELD,
        },
        util::Compression,
    };

    #[tokio::test]
    async fn test_build_healthcheck_200_response_returns_ok() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/services/collector/health/1.0"))
            .and(header("Authorization", "Splunk token"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = create_client(&None, &ProxyConfig::default()).unwrap();
        let healthcheck = build_healthcheck(mock_server.uri(), "token".to_string(), client);

        assert!(healthcheck.await.is_ok())
    }

    #[tokio::test]
    async fn test_build_healthcheck_400_response_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/services/collector/health/1.0"))
            .and(header("Authorization", "Splunk token"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&mock_server)
            .await;

        let client = create_client(&None, &ProxyConfig::default()).unwrap();
        let healthcheck = build_healthcheck(mock_server.uri(), "token".to_string(), client);

        assert_eq!(
            &healthcheck.await.unwrap_err().to_string(),
            "Invalid HEC token"
        );
    }

    #[tokio::test]
    async fn test_build_healthcheck_503_response_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/services/collector/health/1.0"))
            .and(header("Authorization", "Splunk token"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock_server)
            .await;

        let client = create_client(&None, &ProxyConfig::default()).unwrap();
        let healthcheck = build_healthcheck(mock_server.uri(), "token".to_string(), client);

        assert_eq!(
            &healthcheck.await.unwrap_err().to_string(),
            "Queues are full"
        );
    }

    #[tokio::test]
    async fn test_build_healthcheck_500_response_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/services/collector/health/1.0"))
            .and(header("Authorization", "Splunk token"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let client = create_client(&None, &ProxyConfig::default()).unwrap();
        let healthcheck = build_healthcheck(mock_server.uri(), "token".to_string(), client);

        assert_eq!(
            &healthcheck.await.unwrap_err().to_string(),
            "Unexpected status: 500 Internal Server Error"
        );
    }

    #[tokio::test]
    async fn test_build_request_compression_none_returns_expected_request() {
        let endpoint = "http://localhost:8888";
        let token = "token";
        let compression = Compression::None;
        let events = Bytes::from("events");
        let http_request_builder = HttpRequestBuilder::new(
            String::from(endpoint),
            EndpointTarget::default(),
            String::from(token),
            compression,
        );

        let request = http_request_builder
            .build_request(
                events.clone(),
                "/services/collector/event",
                None,
                MetadataFields::default(),
                false,
            )
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
        let events = Bytes::from("events");
        let http_request_builder = HttpRequestBuilder::new(
            String::from(endpoint),
            EndpointTarget::default(),
            String::from(token),
            compression,
        );

        let request = http_request_builder
            .build_request(
                events.clone(),
                "/services/collector/event",
                None,
                MetadataFields::default(),
                false,
            )
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
        let events = Bytes::from("events");
        let http_request_builder = HttpRequestBuilder::new(
            String::from(endpoint),
            EndpointTarget::default(),
            String::from(token),
            compression,
        );

        let err = http_request_builder
            .build_request(
                events,
                "/services/collector/event",
                None,
                MetadataFields::default(),
                false,
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "URI parse error: invalid format")
    }

    #[test]
    fn test_build_uri() {
        let query = [
            (HOST_FIELD, "zork flork".to_string()),
            (SOURCE_FIELD, "zam".to_string()),
        ];
        let uri = build_uri("http://sproink.com", "/thing/thang", query).unwrap();

        assert_eq!(
            "http://sproink.com/thing/thang?host=zork%20flork&source=zam"
                .parse::<Uri>()
                .unwrap(),
            uri
        );
    }
}

#[cfg(all(test, feature = "splunk-integration-tests"))]
mod integration_tests {
    use std::net::SocketAddr;

    use http::StatusCode;
    use tokio::time::Duration;
    use vector_lib::config::proxy::ProxyConfig;
    use warp::Filter;

    use super::{
        build_healthcheck, create_client,
        integration_test_helpers::{get_token, splunk_hec_address},
    };
    use crate::{
        assert_downcast_matches, sinks::splunk_hec::common::HealthcheckError,
        test_util::retry_until,
    };

    #[tokio::test]
    async fn splunk_healthcheck_ok() {
        let client = create_client(&None, &ProxyConfig::default()).unwrap();
        let address = splunk_hec_address();
        let token = get_token().await;

        retry_until(
            || build_healthcheck(address.clone(), token.clone(), client.clone()),
            Duration::from_millis(500),
            Duration::from_secs(30),
        )
        .await;
    }

    #[tokio::test]
    async fn splunk_healthcheck_server_not_listening() {
        let client = create_client(&None, &ProxyConfig::default()).unwrap();
        let healthcheck = build_healthcheck(
            "http://localhost:1111/".to_string(),
            get_token().await,
            client,
        );

        healthcheck.await.unwrap_err();
    }

    #[tokio::test]
    async fn splunk_healthcheck_server_unavailable() {
        let client = create_client(&None, &ProxyConfig::default()).unwrap();
        let healthcheck = build_healthcheck(
            "http://localhost:5503/".to_string(),
            get_token().await,
            client,
        );

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

#[cfg(all(test, feature = "splunk-integration-tests"))]
pub mod integration_test_helpers {
    use serde_json::Value as JsonValue;
    use tokio::time::Duration;

    use crate::test_util::retry_until;

    const USERNAME: &str = "admin";
    const PASSWORD: &str = "password";

    pub fn splunk_hec_address() -> String {
        std::env::var("SPLUNK_HEC_ADDRESS").unwrap_or_else(|_| "http://localhost:8088".into())
    }

    pub fn splunk_api_address() -> String {
        std::env::var("SPLUNK_API_ADDRESS").unwrap_or_else(|_| "https://localhost:8089".into())
    }

    pub async fn get_token() -> String {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let res = retry_until(
            || {
                client
                    .get(format!(
                        "{}/services/data/inputs/http?output_mode=json",
                        splunk_api_address()
                    ))
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
