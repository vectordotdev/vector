//! Configuration for the `keep` sink.

use bytes::Bytes;
use futures::FutureExt;
use http::{Request, StatusCode, Uri};
use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;
use vrl::value::Kind;

use crate::{
    http::HttpClient,
    sinks::{
        prelude::*,
        util::{
            http::{http_response_retry_logic, HttpService},
            BatchConfig, BoxedRawValue,
        },
    },
};

use super::{
    encoder::KeepEncoder, request_builder::KeepRequestBuilder, service::KeepSvcRequestBuilder,
    sink::KeepSink,
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
    pub(super) endpoint: String,

    /// The API key that is used to authenticate against Keep.
    #[configurable(metadata(docs::examples = "${KEEP_API_KEY}"))]
    #[configurable(metadata(docs::examples = "keepappkey"))]
    api_key: SensitiveString,

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig<KeepDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    encoding: Transformer,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn default_endpoint() -> String {
    "http://localhost:8080/alerts/event/vectordev?provider_id=test".to_string()
}

#[derive(Clone, Copy, Debug, Default)]
struct KeepDefaultBatchSettings;

impl SinkBatchSettings for KeepDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(100_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

impl GenerateConfig for KeepConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"api_key = "${KEEP_API_KEY}"
            "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "keep")]
impl SinkConfig for KeepConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let request_builder = KeepRequestBuilder {
            encoder: KeepEncoder {
                transformer: self.encoding.clone(),
            },
            // TODO: add compression support
            compression: Compression::None,
        };

        let uri: Uri = self.endpoint.clone().try_into()?;
        let keep_service_request_builder = KeepSvcRequestBuilder {
            uri: uri.clone(),
            api_key: self.api_key.clone(),
        };

        let client = HttpClient::new(None, cx.proxy())?;

        let service = HttpService::new(client.clone(), keep_service_request_builder);

        let request_limits = self.request.into_settings();

        let service = ServiceBuilder::new()
            .settings(request_limits, http_response_retry_logic())
            .service(service);

        let sink = KeepSink::new(service, batch_settings, request_builder);

        let healthcheck = healthcheck(uri, self.api_key.clone(), client).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        let requirement = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
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
    let body = hyper::body::to_bytes(res.into_body()).await?;

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
            Err(format!(
                "Server returned unexpected error status: {} body: {}",
                status, body
            )
            .into())
        }
    }
}
