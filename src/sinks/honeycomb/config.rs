use bytes::Bytes;
use futures::FutureExt;
use http::StatusCode;
use http_1::{HeaderValue, Request};
use http_body_util::BodyExt;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};
use vrl::value::Kind;

use super::{
    encoder::HoneycombEncoder, request_builder::HoneycombRequestBuilder,
    service::HoneycombSvcRequestBuilder, sink::HoneycombSink,
};
use crate::{
    config::ValidatedSink,
    http::client_v1::{HttpClient, full_body},
    sinks::{
        prelude::*,
        util::{
            BatchConfig, BoxedRawValue, HttpEndpoint, TowerRequestSettings,
            http::RetryStrategy,
            http_v1::{HttpService, http_response_retry_logic},
        },
    },
    tls::MaybeTlsSettings,
};

pub(super) const HTTP_HEADER_HONEYCOMB: &str = "X-Honeycomb-Team";

/// Configuration for the `honeycomb` sink.
#[configurable_component(sink("honeycomb", "Deliver log events to Honeycomb."))]
#[derive(Clone, Debug)]
pub struct HoneycombConfig {
    /// Honeycomb's endpoint to send logs to
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(
        docs::examples = "https://api.honeycomb.io",
        docs::examples = "https://api.eu1.honeycomb.io",
    ))]
    #[configurable(validation(format = "uri"))]
    pub(super) endpoint: HttpEndpoint,

    /// The API key that is used to authenticate against Honeycomb.
    #[configurable(metadata(docs::examples = "${HONEYCOMB_API_KEY}"))]
    #[configurable(metadata(docs::examples = "some-api-key"))]
    api_key: SensitiveString,

    /// The dataset to which logs are sent.
    #[configurable(metadata(docs::examples = "my-honeycomb-dataset"))]
    // TODO: we probably want to make this a template
    // but this limits us in how we can do our healthcheck.
    dataset: String,

    #[serde(default)]
    batch: BatchConfig<HoneycombDefaultBatchSettings>,

    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    encoding: Transformer,

    /// The compression algorithm to use.
    #[serde(default = "Compression::zstd_default")]
    compression: Compression,

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
    HttpEndpoint::parse("https://api.honeycomb.io")
        .expect("static default endpoint should be a valid http(s) URL")
}

#[derive(Clone, Copy, Debug, Default)]
struct HoneycombDefaultBatchSettings;

impl SinkBatchSettings for HoneycombDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(100_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

impl GenerateConfig for HoneycombConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r#"api_key: ${HONEYCOMB_API_KEY}
            dataset: my-honeycomb-dataset"#,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "honeycomb")]
impl SinkConfig for HoneycombConfig {
    fn input(&self) -> Input {
        let requirement = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, derive_more::Debug)]
pub struct ValidatedHoneycomb {
    batch_settings: BatcherSettings,
    uri: HttpEndpoint,
    request_limits: TowerRequestSettings,
    // Omitted: `api_key` is sent as the `X-Honeycomb-Team` header on every
    // request.
    #[debug(skip)]
    api_key: HeaderValue,
}

#[async_trait::async_trait]
impl ValidatedSink for HoneycombConfig {
    type Validated = ValidatedHoneycomb;
    fn validate(&self) -> crate::Result<ValidatedHoneycomb> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;
        let uri = self.build_uri()?;
        let request_limits = self.request.into_settings();
        // The API key becomes the `X-Honeycomb-Team` header on every request.
        let api_key = HeaderValue::from_str(self.api_key.inner())
            .map_err(|e| format!("invalid `api_key`: {e}"))?;

        Ok(ValidatedHoneycomb {
            batch_settings,
            uri,
            request_limits,
            api_key,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedHoneycomb,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedHoneycomb {
            batch_settings,
            uri,
            request_limits,
            api_key,
        } = validated;

        let request_builder = HoneycombRequestBuilder {
            encoder: HoneycombEncoder {
                transformer: self.encoding.clone(),
            },
            compression: self.compression,
        };

        let honeycomb_service_request_builder = HoneycombSvcRequestBuilder {
            uri: uri.clone(),
            api_key: api_key.clone(),
            compression: self.compression,
        };

        let client = HttpClient::new(MaybeTlsSettings::from_config(None, false)?, cx.proxy())?;

        let service = HttpService::new(client.clone(), honeycomb_service_request_builder);

        let service = ServiceBuilder::new()
            .settings(
                request_limits.clone(),
                http_response_retry_logic(self.retry_strategy.clone()),
            )
            .service(service);

        let sink = HoneycombSink::new(service, *batch_settings, request_builder);

        let healthcheck = healthcheck(uri.clone(), api_key.clone(), client).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

impl HoneycombConfig {
    fn build_uri(&self) -> crate::Result<HttpEndpoint> {
        Ok(self
            .endpoint
            .append_path(&format!("1/batch/{}", self.dataset))?)
    }
}
async fn healthcheck(
    uri: HttpEndpoint,
    api_key: HeaderValue,
    client: HttpClient,
) -> crate::Result<()> {
    let request = Request::post(uri.clone().into_v1()).header(HTTP_HEADER_HONEYCOMB, api_key);
    let body = crate::serde::json::to_bytes(&Vec::<BoxedRawValue>::new())
        .unwrap()
        .freeze();
    let req: Request<Bytes> = request.body(body)?;
    let req = req.map(full_body);

    let res = client.send(req).await?;

    let status = StatusCode::from_u16(res.status().as_u16())
        .expect("HTTP status codes are valid u16 values");
    let body = res.into_body().collect().await?.to_bytes();

    if status == StatusCode::BAD_REQUEST {
        Ok(())
    } else if status == StatusCode::UNAUTHORIZED {
        let json: serde_json::Value = serde_json::from_slice(&body[..])?;

        let message = if let Some(s) = json
            .as_object()
            .and_then(|o| o.get("error"))
            .and_then(|s| s.as_str())
        {
            s.to_string()
        } else {
            "Token is not valid, 401 returned.".to_string()
        };

        Err(message.into())
    } else {
        let body = String::from_utf8_lossy(&body[..]);

        Err(format!("Server returned unexpected error status: {status} body: {body}").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_returns_usable_values() {
        let config: HoneycombConfig = serde_json::from_value(HoneycombConfig::generate_config())
            .expect("config should be valid");

        let validated = config.validate().expect("validation should succeed");
        assert_eq!(
            validated.uri.to_string(),
            "https://api.honeycomb.io/1/batch/my-honeycomb-dataset"
        );
        // Default batch settings from `HoneycombDefaultBatchSettings`.
        assert_eq!(
            validated.batch_settings.timeout,
            std::time::Duration::from_secs(1)
        );
    }

    #[test]
    fn validate_rejects_invalid_api_key() {
        // A key with a newline cannot be a header value.
        let config = HoneycombConfig {
            api_key: "key\nwith_newline".to_string().into(),
            ..serde_json::from_value(HoneycombConfig::generate_config())
                .expect("config should be valid")
        };

        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("invalid `api_key`"), "unexpected error: {err}");
    }
}
