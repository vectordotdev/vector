//! Configuration for the `opsimate` sink.

use futures::FutureExt;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use vector_http::empty_body;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};
use vrl::value::Kind;

use super::{
    encoder::OpsiMateEncoder, request_builder::OpsiMateRequestBuilder,
    service::OpsiMateSvcRequestBuilder, sink::OpsiMateSink,
};
use crate::{
    config::ValidatedSink,
    http::HttpClient,
    sinks::{
        prelude::*,
        util::{
            BatchConfig, HttpEndpoint, TowerRequestSettings,
            http::{HttpService, RetryStrategy, http_response_retry_logic},
        },
    },
};

pub(super) const HTTP_HEADER_OPSIMATE_API_TOKEN: &str = "x-api-token";

/// Path of the custom alerts endpoint, relative to the configured server base URL.
const ALERTS_PATH: &str = "/api/v1/alerts/custom";

/// Path of the unauthenticated health endpoint, relative to the configured server base URL.
const HEALTH_PATH: &str = "/health";

/// OpsiMate's custom alerts endpoint accepts exactly one alert per request.
const MAX_EVENTS_PER_REQUEST: usize = 1;

/// Configuration for the `opsimate` sink.
#[configurable_component(sink("opsimate", "Deliver log events to OpsiMate as alerts."))]
#[derive(Clone, Debug)]
pub(super) struct OpsiMateConfig {
    /// The base URL of the OpsiMate server.
    ///
    /// Alerts are sent to `{endpoint}/api/v1/alerts/custom` and the health check
    /// queries `{endpoint}/health`.
    #[configurable(metadata(docs::examples = "http://opsimate.example.com:3001"))]
    #[configurable(validation(format = "uri"))]
    pub(super) endpoint: HttpEndpoint,

    /// The API token that is used to authenticate against OpsiMate.
    ///
    /// It is sent in the `x-api-token` header of every request.
    #[configurable(metadata(docs::examples = "${OPSIMATE_API_TOKEN}"))]
    api_token: SensitiveString,

    #[serde(default)]
    batch: BatchConfig<OpsiMateDefaultBatchSettings>,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    encoding: Transformer,

    tls: Option<TlsConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    #[serde(default)]
    pub retry_strategy: RetryStrategy,
}

#[derive(Clone, Copy, Debug, Default)]
struct OpsiMateDefaultBatchSettings;

impl SinkBatchSettings for OpsiMateDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(MAX_EVENTS_PER_REQUEST);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

impl GenerateConfig for OpsiMateConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r"endpoint: http://opsimate.example.com:3001
            api_token: ${OPSIMATE_API_TOKEN}
            ",
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "opsimate")]
impl SinkConfig for OpsiMateConfig {
    fn input(&self) -> Input {
        let requirement = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub(super) struct ValidatedOpsiMate {
    batch_settings: BatcherSettings,
    alerts_endpoint: HttpEndpoint,
    health_endpoint: HttpEndpoint,
    request_limits: TowerRequestSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for OpsiMateConfig {
    type Validated = ValidatedOpsiMate;

    fn validate(&self) -> crate::Result<ValidatedOpsiMate> {
        let batch_settings = self
            .batch
            .validate()?
            .limit_max_events(MAX_EVENTS_PER_REQUEST)
            .map_err(|error| {
                format!(
                    "`batch.max_events` must be {MAX_EVENTS_PER_REQUEST}: \
                     the OpsiMate custom alerts API accepts one alert per request ({error})"
                )
            })?
            .into_batcher_settings()?;
        let alerts_endpoint = self.endpoint.append_path(ALERTS_PATH)?;
        let health_endpoint = self.endpoint.append_path(HEALTH_PATH)?;
        let request_limits = self.request.into_settings();

        Ok(ValidatedOpsiMate {
            batch_settings,
            alerts_endpoint,
            health_endpoint,
            request_limits,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedOpsiMate,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedOpsiMate {
            batch_settings,
            alerts_endpoint,
            health_endpoint,
            request_limits,
        } = validated;

        let request_builder = OpsiMateRequestBuilder {
            encoder: OpsiMateEncoder {
                transformer: self.encoding.clone(),
            },
            compression: Compression::None,
        };

        let service_request_builder = OpsiMateSvcRequestBuilder {
            alerts_endpoint: alerts_endpoint.clone(),
            api_token: self.api_token.clone(),
        };

        let tls = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls.into(), cx.proxy())?;

        let service = HttpService::new(client.clone(), service_request_builder);

        let service = ServiceBuilder::new()
            .settings(
                request_limits.clone(),
                http_response_retry_logic(self.retry_strategy.clone()),
            )
            .service(service);

        let sink = OpsiMateSink::new(service, *batch_settings, request_builder);

        let healthcheck = healthcheck(health_endpoint.clone(), client).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

/// Queries OpsiMate's unauthenticated `/health` endpoint; any `200 OK` means healthy.
async fn healthcheck(health_endpoint: HttpEndpoint, client: HttpClient) -> crate::Result<()> {
    let request = Request::get(health_endpoint.into_uri()).body(empty_body())?;

    let res = client.send(request).await?;

    let status = res.status();
    if status == StatusCode::OK {
        return Ok(());
    }

    let body = res.into_body().collect().await?.to_bytes();
    let body = String::from_utf8_lossy(&body[..]);
    Err(format!("OpsiMate health check returned unexpected status: {status} body: {body}").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ValidatedSink;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<OpsiMateConfig>();
    }

    #[test]
    fn rejects_non_http_endpoint() {
        let err = serde_yaml::from_str::<OpsiMateConfig>(
            r#"
            api_token: "test-token"
            endpoint: "ftp://example.com"
            "#,
        )
        .expect_err("a non-http endpoint must be rejected");
        assert!(err.to_string().contains("http"), "unexpected error: {err}");
    }

    #[test]
    fn validate_resolves_alert_and_health_endpoints() {
        let config: OpsiMateConfig = serde_yaml::from_str(
            r#"
            api_token: "test-token"
            endpoint: "http://opsimate.example.com:3001"
            "#,
        )
        .unwrap();
        let validated = config.validate().expect("validation should succeed");
        assert_eq!(
            validated.alerts_endpoint.to_string(),
            "http://opsimate.example.com:3001/api/v1/alerts/custom"
        );
        assert_eq!(
            validated.health_endpoint.to_string(),
            "http://opsimate.example.com:3001/health"
        );
        assert_eq!(validated.batch_settings.item_limit, 1);
    }

    #[test]
    fn validate_strips_trailing_slash_from_endpoint() {
        let config: OpsiMateConfig = serde_yaml::from_str(
            r#"
            api_token: "test-token"
            endpoint: "https://opsimate.example.com/"
            "#,
        )
        .unwrap();
        let validated = config.validate().expect("validation should succeed");
        assert_eq!(
            validated.alerts_endpoint.to_string(),
            "https://opsimate.example.com/api/v1/alerts/custom"
        );
        assert_eq!(
            validated.health_endpoint.to_string(),
            "https://opsimate.example.com/health"
        );
    }

    #[test]
    fn rejects_batches_of_more_than_one_event() {
        let config: OpsiMateConfig = serde_yaml::from_str(
            r#"
            api_token: "test-token"
            endpoint: "http://opsimate.example.com:3001"
            batch:
              max_events: 10
            "#,
        )
        .unwrap();
        let err = config
            .validate()
            .expect_err("more than one event per request must be rejected");
        let message = err.to_string();
        assert!(
            message.contains("`batch.max_events` must be 1"),
            "unexpected error: {message}"
        );
    }

    #[test]
    fn accepts_an_explicit_max_events_of_one() {
        let config: OpsiMateConfig = serde_yaml::from_str(
            r#"
            api_token: "test-token"
            endpoint: "http://opsimate.example.com:3001"
            batch:
              max_events: 1
            "#,
        )
        .unwrap();
        let validated = config
            .validate()
            .expect("max_events of 1 is the only valid value");
        assert_eq!(validated.batch_settings.item_limit, 1);
    }
}
