//! Configuration for the `honeycomb` sink.

use bytes::Bytes;
use futures::FutureExt;
use http_1::{Request, StatusCode};
use http_body_util::BodyExt;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};
use vrl::value::Kind;

use super::{
    encoder::HoneycombEncoder, request_builder::HoneycombRequestBuilder,
    service::HoneycombSvcRequestBuilderV1, sink::HoneycombSink,
};
use crate::{
    http::{client_v1::HttpClientV1, client_v1::full_body},
    sinks::{
        prelude::*,
        util::{
            BatchConfig, BoxedRawValue, HttpEndpoint,
            http::RetryStrategy,
            http_v1::{HttpServiceV1, http_response_retry_logic_v1},
        },
    },
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

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig<HoneycombDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    encoding: Transformer,

    /// The compression algorithm to use.
    #[configurable(derived)]
    #[serde(default = "Compression::zstd_default")]
    compression: Compression,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
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
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let request_builder = HoneycombRequestBuilder {
            encoder: HoneycombEncoder {
                transformer: self.encoding.clone(),
            },
            compression: self.compression,
        };

        let uri = self.build_uri()?;

        let honeycomb_service_request_builder = HoneycombSvcRequestBuilderV1 {
            uri: uri.clone(),
            api_key: self.api_key.clone(),
            compression: self.compression,
        };

        let client = HttpClientV1::new(None.into(), cx.proxy())?;

        let service = HttpServiceV1::new(client.clone(), honeycomb_service_request_builder);

        let request_limits = self.request.into_settings();

        let service = ServiceBuilder::new()
            .settings(
                request_limits,
                http_response_retry_logic_v1(self.retry_strategy.clone()),
            )
            .service(service);

        let sink = HoneycombSink::new(service, batch_settings, request_builder);

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

impl HoneycombConfig {
    fn build_uri(&self) -> crate::Result<HttpEndpoint> {
        Ok(self
            .endpoint
            .append_path(&format!("1/batch/{}", self.dataset))?)
    }
}

async fn healthcheck(
    uri: HttpEndpoint,
    api_key: SensitiveString,
    client: HttpClientV1,
) -> crate::Result<()> {
    let request = Request::post(uri.to_string()).header(HTTP_HEADER_HONEYCOMB, api_key.inner());
    let body = crate::serde::json::to_bytes(&Vec::<BoxedRawValue>::new())
        .unwrap()
        .freeze();
    let req: Request<Bytes> = request.body(body)?;
    let req = req.map(full_body);

    let res = client.send(req).await?;

    let status = res.status();
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
