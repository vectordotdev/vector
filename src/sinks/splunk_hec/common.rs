use crate::{
    http::HttpClient,
    internal_events::TemplateRenderingFailed,
    sinks::{self, util::Compression, UriParseError},
    template::Template,
    tls::{TlsOptions, TlsSettings},
};
use http::{Request, StatusCode, Uri};
use hyper::Body;
use snafu::{ResultExt, Snafu};
use vector_core::{config::proxy::ProxyConfig, event::EventRef};

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Invalid HEC token"))]
    InvalidToken,
    #[snafu(display("Queues are full"))]
    QueuesFull,
}

pub fn create_client(
    tls: &Option<TlsOptions>,
    proxy_config: &ProxyConfig,
) -> crate::Result<HttpClient> {
    let tls_settings = TlsSettings::from_options(tls)?;
    Ok(HttpClient::new(tls_settings, proxy_config)?)
}

pub async fn build_healthcheck(
    endpoint: String,
    token: String,
    client: HttpClient,
) -> crate::Result<()> {
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

pub fn build_uri(host: &str, path: &str) -> Result<Uri, http::uri::InvalidUri> {
    format!("{}{}", host.trim_end_matches('/'), path).parse::<Uri>()
}

pub fn host_key() -> String {
    crate::config::log_schema().host_key().to_string()
}

pub fn render_template_string<'a>(
    template: &Template,
    event: impl Into<EventRef<'a>>,
    field_name: &str,
) -> Option<String> {
    template
        .render_string(event)
        .map_err(|error| {
            emit!(&TemplateRenderingFailed {
                error,
                field: Some(field_name),
                drop_event: false
            });
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use vector_core::config::proxy::ProxyConfig;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::sinks::splunk_hec::common::build_healthcheck;

    use super::create_client;

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
}
