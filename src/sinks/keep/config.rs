//! Configuration for the `keep` sink.

use bytes::Bytes;
use futures::FutureExt;
use http::{Request, StatusCode, Uri};
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};
use vrl::value::Kind;

use super::{
    encoder::KeepEncoder, request_builder::KeepRequestBuilder, service::KeepSvcRequestBuilder,
    sink::KeepSink,
};
use crate::{
    config::ValidatedSink,
    http::HttpClient,
    sinks::{
        prelude::*,
        util::{
            BatchConfig, BoxedRawValue, HttpEndpoint, TowerRequestSettings,
            http::{HttpService, RetryStrategy, http_response_retry_logic},
        },
    },
};

pub(super) const HTTP_HEADER_KEEP_API_KEY: &str = "x-api-key";

/// Configuration for the `keep` sink.
#[configurable_component(sink("keep", "Deliver log events to Keep."))]
#[derive(Clone, Debug)]
pub struct KeepConfig {
    /// Keeps endpoint to send logs to
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(
        docs::examples = "https://backend.keep.com:8081/alerts/event/vectordev?provider_id=test",
    ))]
    #[configurable(validation(format = "uri"))]
    pub(super) endpoint: HttpEndpoint,

    /// The API key that is used to authenticate against Keep.
    #[configurable(metadata(docs::examples = "${KEEP_API_KEY}"))]
    #[configurable(metadata(docs::examples = "keepappkey"))]
    api_key: SensitiveString,

    #[serde(default)]
    batch: BatchConfig<KeepDefaultBatchSettings>,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    encoding: Transformer,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    #[serde(default)]
    pub retry_strategy: RetryStrategy,
}

fn default_endpoint() -> HttpEndpoint {
    HttpEndpoint::parse("http://localhost:8080/alerts/event/vectordev?provider_id=test")
        .expect("static default endpoint should be a valid http(s) URL")
}

#[derive(Clone, Copy, Debug, Default)]
struct KeepDefaultBatchSettings;

impl SinkBatchSettings for KeepDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(100_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

impl GenerateConfig for KeepConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r#"api_key: ${KEEP_API_KEY}
            "#,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "keep")]
impl SinkConfig for KeepConfig {
    fn input(&self) -> Input {
        let requirement = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedKeep {
    batch_settings: BatcherSettings,
    uri: Uri,
    request_limits: TowerRequestSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for KeepConfig {
    type Validated = ValidatedKeep;

    fn validate(&self) -> crate::Result<ValidatedKeep> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;
        let uri = self.endpoint.clone().into_uri();
        let request_limits = self.request.into_settings();

        Ok(ValidatedKeep {
            batch_settings,
            uri,
            request_limits,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedKeep,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedKeep {
            batch_settings,
            uri,
            request_limits,
        } = validated;

        let request_builder = KeepRequestBuilder {
            encoder: KeepEncoder {
                transformer: self.encoding.clone(),
            },
            // TODO: add compression support
            compression: Compression::None,
        };

        let keep_service_request_builder = KeepSvcRequestBuilder {
            uri: uri.clone(),
            api_key: self.api_key.clone(),
        };

        let client = HttpClient::new(None, cx.proxy())?;

        let service = HttpService::new(client.clone(), keep_service_request_builder);

        let service = ServiceBuilder::new()
            .settings(
                request_limits.clone(),
                http_response_retry_logic(self.retry_strategy.clone()),
            )
            .service(service);

        let sink = KeepSink::new(service, *batch_settings, request_builder);

        let healthcheck = healthcheck(uri.clone(), self.api_key.clone(), client).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

async fn healthcheck(uri: Uri, api_key: SensitiveString, client: HttpClient) -> crate::Result<()> {
    let request = Request::post(uri).header(HTTP_HEADER_KEEP_API_KEY, api_key.inner());
    let body = crate::serde::json::to_bytes(&Vec::<BoxedRawValue>::new())
        .unwrap()
        .freeze();
    let req: Request<Bytes> = request.body(body)?;
    let req = req.map(hyper::Body::from);

    let res = client.send(req).await?;

    let status = res.status();
    let body = http_body::Body::collect(res.into_body()).await?.to_bytes();

    match status {
        StatusCode::OK => Ok(()),          // Healthcheck passed
        StatusCode::BAD_REQUEST => Ok(()), // Healthcheck failed due to client error but is still considered valid
        StatusCode::ACCEPTED => Ok(()),    // Consider healthcheck passed if server accepted request
        StatusCode::UNAUTHORIZED => {
            // Handle unauthorized errors
            let json: serde_json::Value = serde_json::from_slice(&body[..])?;
            let message = json
                .as_object()
                .and_then(|o| o.get("error"))
                .and_then(|s| s.as_str())
                .unwrap_or("Token is not valid, 401 returned.")
                .to_string();
            Err(message.into())
        }
        _ => {
            // Handle other unexpected statuses
            let body = String::from_utf8_lossy(&body[..]);
            Err(format!("Server returned unexpected error status: {status} body: {body}").into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ValidatedSink;

    #[test]
    fn rejects_non_http_endpoint() {
        let err = serde_yaml::from_str::<KeepConfig>(
            r#"
            api_key: "test-key"
            endpoint: "ftp://example.com"
            "#,
        )
        .expect_err("a non-http endpoint must be rejected");
        assert!(err.to_string().contains("http"), "unexpected error: {err}");
    }

    #[test]
    fn validate_produces_usable_values() {
        let config: KeepConfig = serde_yaml::from_str(
            r#"
            api_key: "test-key"
            "#,
        )
        .unwrap();
        let validated = config.validate().expect("validation should succeed");
        assert_eq!(
            validated.uri.to_string(),
            "http://localhost:8080/alerts/event/vectordev?provider_id=test"
        );
        assert_eq!(validated.batch_settings.size_limit, 100_000);
    }
}
