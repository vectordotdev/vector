use bytes::Bytes;
use futures::FutureExt;
use http::{Request, StatusCode, Uri};
use vector_common::sensitive_string::SensitiveString;
use vrl::value::Kind;

use crate::{
    http::HttpClient,
    sinks::{
        prelude::*,
        util::{
            http::{HttpResponse, HttpService, HttpStatusRetryLogic},
            BatchConfig, BoxedRawValue,
        },
    },
};

use super::{
    encoder::HoneycombEncoder, request_builder::HoneycombRequestBuilder,
    service::HoneycombSvcRequestBuilder, sink::HoneycombSink,
};

pub(super) const HTTP_HEADER_HONEYCOMB: &str = "X-Honeycomb-Team";

/// Configuration for the `honeycomb` sink.
#[configurable_component(sink("honeycomb", "Deliver log events to Honeycomb."))]
#[derive(Clone, Debug)]
pub struct HoneycombConfig {
    // This endpoint is not user-configurable and only exists for testing purposes
    #[serde(skip, default = "default_endpoint")]
    pub(super) endpoint: String,

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
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    encoding: Transformer,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn default_endpoint() -> String {
    "https://api.honeycomb.io/1/batch".to_string()
}

#[derive(Clone, Copy, Debug, Default)]
struct HoneycombDefaultBatchSettings;

impl SinkBatchSettings for HoneycombDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(100_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

impl GenerateConfig for HoneycombConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"api_key = "${HONEYCOMB_API_KEY}"
            dataset = "my-honeycomb-dataset""#,
        )
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
        };

        let honeycomb_service_request_builder = HoneycombSvcRequestBuilder {
            uri: self.build_uri(),
            api_key: self.api_key.clone(),
        };

        let client = HttpClient::new(None, cx.proxy())?;

        let service = HttpService::new(client.clone(), honeycomb_service_request_builder);

        let request_limits = self.request.unwrap_with(&TowerRequestConfig::default());

        let retry_logic =
            HttpStatusRetryLogic::new(|req: &HttpResponse| req.http_response.status());

        let service = ServiceBuilder::new()
            .settings(request_limits, retry_logic)
            .service(service);

        let sink = HoneycombSink::new(service, batch_settings, request_builder);

        let healthcheck = healthcheck(self.build_request(Vec::new())?, client).boxed();

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
    fn build_uri(&self) -> Uri {
        let uri = format!("{}/{}", self.endpoint, self.dataset);

        uri.parse::<Uri>().expect("This should be a valid uri")
    }

    fn build_request(&self, events: Vec<BoxedRawValue>) -> crate::Result<Request<Bytes>> {
        let uri = self.build_uri();
        let request = Request::post(uri).header(HTTP_HEADER_HONEYCOMB, self.api_key.inner());
        let body = crate::serde::json::to_bytes(&events).unwrap().freeze();

        request.body(body).map_err(Into::into)
    }
}

async fn healthcheck(req: Request<Bytes>, client: HttpClient) -> crate::Result<()> {
    let req = req.map(hyper::Body::from);

    let res = client.send(req).await?;

    let status = res.status();
    let body = hyper::body::to_bytes(res.into_body()).await?;

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

        Err(format!(
            "Server returned unexpected error status: {} body: {}",
            status, body
        )
        .into())
    }
}
