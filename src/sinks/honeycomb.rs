use bytes::{Bytes, BytesMut};
use futures::{FutureExt, SinkExt};
use http::{Request, StatusCode, Uri};
use indoc::indoc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::codec::Encoder as _;

use crate::{
    codecs::{Encoder, EncodingConfig},
    config::{
        log_schema, AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext,
        SinkDescription,
    },
    event::{Event, Value},
    http::HttpClient,
    sinks::util::{
        encoding::Transformer,
        http::{BatchedHttpSink, HttpEventEncoder, HttpSink},
        BatchConfig, BoxedRawValue, JsonArrayBuffer, SinkBatchSettings, TowerRequestConfig,
    },
};

static HOST: Lazy<Uri> = Lazy::new(|| Uri::from_static("https://api.honeycomb.io/1/batch"));

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HoneycombConfig {
    api_key: String,
    // TODO: we probably want to make this a template
    // but this limits us in how we can do our healthcheck.
    dataset: String,
    #[serde(default)]
    batch: BatchConfig<HoneycombDefaultBatchSettings>,
    #[serde(default)]
    request: TowerRequestConfig,
    encoding: EncodingConfig,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

#[derive(Clone, Copy, Debug, Default)]
struct HoneycombDefaultBatchSettings;

impl SinkBatchSettings for HoneycombDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(100_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

inventory::submit! {
    SinkDescription::new::<HoneycombConfig>("honeycomb")
}

impl GenerateConfig for HoneycombConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            api_key = "${HONEYCOMB_API_KEY}"
            dataset = "my-honeycomb-dataset"
            encoding.codec = "json"
        "#})
        .unwrap()
    }
}

#[derive(Debug, Clone)]
struct HoneycombSink {
    api_key: String,
    dataset: String,
    transformer: Transformer,
    encoder: Encoder<()>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "honeycomb")]
impl SinkConfig for HoneycombConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = self.batch.into_batch_settings()?;

        let buffer = JsonArrayBuffer::new(batch_settings.size);

        let client = HttpClient::new(None, cx.proxy())?;

        let transformer = self.encoding.transformer();
        let serializer = self.encoding.config().build();
        let encoder = Encoder::<()>::new(serializer);

        let sink = HoneycombSink {
            api_key: self.api_key.clone(),
            dataset: self.dataset.clone(),
            transformer,
            encoder,
        };

        let healthcheck = healthcheck(sink.clone(), client.clone()).boxed();

        let sink = BatchedHttpSink::new(
            sink,
            buffer,
            request_settings,
            batch_settings.timeout,
            client,
            cx.acker(),
        )
        .sink_map_err(|error| error!(message = "Fatal honeycomb sink error.", %error));

        Ok((super::VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type())
    }

    fn sink_type(&self) -> &'static str {
        "honeycomb"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

pub struct HoneycombEventEncoder {
    transformer: Transformer,
    encoder: Encoder<()>,
}

impl HttpEventEncoder<serde_json::Value> for HoneycombEventEncoder {
    fn encode_event(&mut self, mut event: Event) -> Option<serde_json::Value> {
        self.transformer.transform(&mut event);

        let timestamp = match &mut event {
            Event::Log(ref mut log) => {
                if let Some(Value::Timestamp(ts)) = log.remove(log_schema().timestamp_key()) {
                    ts
                } else {
                    chrono::Utc::now()
                }
            }
            Event::Metric(_) | Event::Trace(_) => chrono::Utc::now(),
        };

        let serializer = self.encoder.serializer();
        let data = if serializer.supports_json() {
            serializer.to_json_value(event).ok()?
        } else {
            let mut bytes = BytesMut::new();
            self.encoder.encode(event, &mut bytes).ok()?;
            String::from_utf8_lossy(&bytes).into()
        };

        let json = json!({
            "timestamp": timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "data": data,
        });

        Some(json)
    }
}

#[async_trait::async_trait]
impl HttpSink for HoneycombSink {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;
    type Encoder = HoneycombEventEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        HoneycombEventEncoder {
            transformer: self.transformer.clone(),
            encoder: self.encoder.clone(),
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Bytes>> {
        let uri = self.build_uri();
        let request = Request::post(uri).header("X-Honeycomb-Team", self.api_key.clone());
        let body = crate::serde::json::to_bytes(&events).unwrap().freeze();

        request.body(body).map_err(Into::into)
    }
}

impl HoneycombSink {
    fn build_uri(&self) -> Uri {
        let uri = format!("{}/{}", HOST.clone(), self.dataset);

        uri.parse::<http::Uri>()
            .expect("This should be a valid uri")
    }
}

async fn healthcheck(sink: HoneycombSink, client: HttpClient) -> crate::Result<()> {
    let req = sink.build_request(Vec::new()).await?.map(hyper::Body::from);

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
    use chrono::{DateTime, Utc};
    use codecs::JsonSerializer;
    use vector_core::event::LogEvent;

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::HoneycombConfig>();
    }

    #[test]
    fn encoder_takes_timestamp_from_event() {
        let mut log = LogEvent::default();
        log.insert_flat(
            "timestamp",
            DateTime::parse_from_rfc3339("2000-01-01T01:02:03.456+00:00")
                .unwrap()
                .with_timezone(&Utc),
        );
        log.insert_flat("foo", 123);
        let event = Event::from(log);

        let mut encoder = HoneycombEventEncoder {
            transformer: Transformer::default(),
            encoder: Encoder::<()>::new(JsonSerializer::new().into()),
        };

        let json = encoder.encode_event(event).unwrap();

        assert_eq!(
            json,
            json!({
                "timestamp": "2000-01-01T01:02:03.456000000Z",
                "data": {
                    "foo": 123
                },
            })
        )
    }
}
