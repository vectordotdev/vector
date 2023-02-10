use bytes::Bytes;
use futures::{FutureExt, SinkExt};
use http::{Request, StatusCode, Uri};
use serde_json::json;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;

use crate::{
    codecs::Transformer,
    config::{log_schema, AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    event::{Event, Value},
    http::HttpClient,
    sinks::util::{
        http::{BatchedHttpSink, HttpEventEncoder, HttpSink},
        BatchConfig, BoxedRawValue, JsonArrayBuffer, SinkBatchSettings, TowerRequestConfig,
    },
};

/// Configuration for the `honeycomb` sink.
#[configurable_component(sink("honeycomb"))]
#[derive(Clone, Debug)]
pub struct HoneycombConfig {
    // This endpoint is not user-configurable and only exists for testing purposes
    #[serde(skip, default = "default_endpoint")]
    endpoint: String,

    /// The team key that will be used to authenticate against Honeycomb.
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
impl SinkConfig for HoneycombConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = self.batch.into_batch_settings()?;

        let buffer = JsonArrayBuffer::new(batch_settings.size);

        let client = HttpClient::new(None, cx.proxy())?;

        let sink = BatchedHttpSink::new(
            self.clone(),
            buffer,
            request_settings,
            batch_settings.timeout,
            client.clone(),
        )
        .sink_map_err(|error| error!(message = "Fatal honeycomb sink error.", %error));

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

pub struct HoneycombEventEncoder {
    transformer: Transformer,
}

impl HttpEventEncoder<serde_json::Value> for HoneycombEventEncoder {
    fn encode_event(&mut self, mut event: Event) -> Option<serde_json::Value> {
        self.transformer.transform(&mut event);
        let mut log = event.into_log();

        let timestamp = if let Some(Value::Timestamp(ts)) = log.remove(log_schema().timestamp_key())
        {
            ts
        } else {
            chrono::Utc::now()
        };

        let data = json!({
            "time": timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "data": log.convert_to_fields(),
        });

        Some(data)
    }
}

#[async_trait::async_trait]
impl HttpSink for HoneycombConfig {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;
    type Encoder = HoneycombEventEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        HoneycombEventEncoder {
            transformer: self.encoding.clone(),
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Bytes>> {
        let uri = self.build_uri();
        let request = Request::post(uri).header("X-Honeycomb-Team", self.api_key.inner());
        let body = crate::serde::json::to_bytes(&events).unwrap().freeze();

        request.body(body).map_err(Into::into)
    }
}

impl HoneycombConfig {
    fn build_uri(&self) -> Uri {
        let uri = format!("{}/{}", self.endpoint, self.dataset);

        uri.parse::<Uri>().expect("This should be a valid uri")
    }
}

async fn healthcheck(config: HoneycombConfig, client: HttpClient) -> crate::Result<()> {
    let req = config
        .build_request(Vec::new())
        .await?
        .map(hyper::Body::from);

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
#[cfg(test)]
mod test {
    use futures::{future::ready, stream};
    use serde::Deserialize;
    use vector_core::event::{Event, LogEvent};

    use crate::{
        config::{GenerateConfig, SinkConfig, SinkContext},
        test_util::{
            components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS},
            http::{always_200_response, spawn_blackhole_http_server},
        },
    };

    use super::HoneycombConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HoneycombConfig>();
    }

    #[tokio::test]
    async fn component_spec_compliance() {
        let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

        let config = HoneycombConfig::generate_config().to_string();
        let mut config = HoneycombConfig::deserialize(toml::de::ValueDeserializer::new(&config))
            .expect("config should be valid");
        config.endpoint = mock_endpoint.to_string();

        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let event = Event::Log(LogEvent::from("simple message"));
        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;
    }
}
