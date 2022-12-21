use bytes::Bytes;
use futures::FutureExt;
use futures_util::SinkExt;
use http::{Request, StatusCode, Uri};
use hyper::Body;
use serde_json::json;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;

#[cfg(all(test, feature = "apex-integration-tests"))]
mod integration_tests;

use crate::{
    codecs::Transformer,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    event::Event,
    http::HttpClient,
    sinks::util::{
        http::{BatchedHttpSink, HttpEventEncoder, HttpSink},
        BatchConfig, BoxedRawValue, JsonArrayBuffer, RealtimeSizeBasedDefaultBatchSettings,
        TowerRequestConfig, UriSerde,
    },
};

/// Configuration for the `apex` sink.
#[configurable_component(sink("apex"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ApexSinkConfig {
    /// The base URI of the Apex instance.
    ///
    /// `/add_events` is appended to this.
    uri: UriSerde,

    /// The ID of the project to associate reported logs with.
    project_id: String,

    /// The API token to use to authenticate with Apex.
    api_token: SensitiveString,

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for ApexSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"uri = "https://your.apex.domain"
            project_id = "your-apex-project-id"
            api_token = "your-apex-api-token""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for ApexSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = self.batch.into_batch_settings()?;

        let buffer = JsonArrayBuffer::new(batch_settings.size);

        let client = HttpClient::new(None, cx.proxy())?;

        // TODO: Update this to a new-style sink when BatchedHttpSink is deprecated.
        let sink = BatchedHttpSink::new(
            self.clone(),
            buffer,
            request_settings,
            batch_settings.timeout,
            client.clone(),
        )
        .sink_map_err(|error| error!(message = "Fatal apex sink error.", %error));

        let healthcheck = healthcheck(self.clone(), client).boxed();

        Ok((super::VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

pub struct ApexEventEncoder {
    transformer: Transformer,
}

impl HttpEventEncoder<serde_json::Value> for ApexEventEncoder {
    fn encode_event(&mut self, mut event: Event) -> Option<serde_json::Value> {
        self.transformer.transform(&mut event);
        let log = event.into_log();
        let body = json!(&log);

        Some(body)
    }
}

#[async_trait::async_trait]
impl HttpSink for ApexSinkConfig {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;
    type Encoder = ApexEventEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        ApexEventEncoder {
            transformer: Transformer::default(),
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Bytes>> {
        let uri: Uri = self.uri.append_path("/add_events")?.uri;
        let request = Request::post(uri)
            .header(
                "Authorization",
                format!("Bearer {}", self.api_token.inner()),
            )
            .header("Content-Type", "application/json");

        let full_body_string = json!({
            "project_id": self.project_id,
            "events": events
        });
        let body = crate::serde::json::to_bytes(&full_body_string)
            .unwrap()
            .freeze();

        request.body(body).map_err(Into::into)
    }
}

async fn healthcheck(config: ApexSinkConfig, client: HttpClient) -> crate::Result<()> {
    let uri = config.uri.with_default_parts();
    let request = Request::head(&uri.uri)
        .header(
            "Authorization",
            format!("Bearer {}", config.api_token.inner()),
        )
        .body(Body::empty())
        .unwrap();
    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(super::HealthcheckError::UnexpectedStatus { status }.into()),
    }
}
