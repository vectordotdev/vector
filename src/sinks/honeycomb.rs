use std::num::NonZeroU64;

use futures::{FutureExt, SinkExt};
use http::{Request, StatusCode, Uri};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::util::SinkBatchSettings;
use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::{Event, Value},
    http::HttpClient,
    sinks::util::{
        http::{BatchedHttpSink, HttpSink},
        BatchConfig, BoxedRawValue, JsonArrayBuffer, TowerRequestConfig,
    },
};

lazy_static::lazy_static! {
    static ref HOST: Uri = Uri::from_static("https://api.honeycomb.io/1/batch");
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HoneycombConfig {
    api_key: String,

    // TODO: we probably want to make this a template
    // but this limits us in how we can do our healthcheck.
    dataset: String,

    #[serde(default)]
    batch: BatchConfig<HoneycombDefaultBatchSettings>,

    #[serde(default)]
    request: TowerRequestConfig,
}

#[derive(Clone, Copy, Debug, Default)]
struct HoneycombDefaultBatchSettings;

impl SinkBatchSettings for HoneycombDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(100_000);
    const TIMEOUT_SECS: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(1) };
}

inventory::submit! {
    SinkDescription::new::<HoneycombConfig>("honeycomb")
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
            cx.acker(),
        )
        .sink_map_err(|error| error!(message = "Fatal honeycomb sink error.", %error));

        let healthcheck = healthcheck(self.clone(), client).boxed();

        Ok((super::VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "honeycomb"
    }
}

#[async_trait::async_trait]
impl HttpSink for HoneycombConfig {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
        let mut log = event.into_log();

        let timestamp = if let Some(Value::Timestamp(ts)) = log.remove(log_schema().timestamp_key())
        {
            ts
        } else {
            chrono::Utc::now()
        };

        let data = json!({
            "timestamp": timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "data": log.all_fields(),
        });

        Some(data)
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let uri = self.build_uri();
        let request = Request::post(uri).header("X-Honeycomb-Team", self.api_key.clone());

        let buf = serde_json::to_vec(&events).unwrap();

        request.body(buf).map_err(Into::into)
    }
}

impl HoneycombConfig {
    fn build_uri(&self) -> Uri {
        let uri = format!("{}/{}", HOST.clone(), self.dataset);

        uri.parse::<http::Uri>()
            .expect("This should be a valid uri")
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
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::HoneycombConfig>();
    }
}
