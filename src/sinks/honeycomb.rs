use crate::{
    dns::Resolver,
    event::{log_schema, Event, Value},
    sinks::util::{
        http::{BatchedHttpSink, HttpClient, HttpSink},
        service2::TowerRequestConfig,
        BatchBytesConfig, BoxedRawValue, JsonArrayBuffer, UriSerde,
    },
    tls::TlsSettings,
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures::TryFutureExt;
use futures01::Sink;
use http::{Request, StatusCode, Uri};
use serde::{Deserialize, Serialize};
use serde_json::json;

lazy_static::lazy_static! {
    static ref HOST: UriSerde = Uri::from_static("https://api.honeycomb.io/1/batch").into();
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HoneycombConfig {
    api_key: String,

    // TODO: we probably want to make this a template
    // but this limits us in how we can do our healthcheck.
    dataset: String,

    #[serde(default)]
    batch: BatchBytesConfig,

    #[serde(default)]
    request: TowerRequestConfig,
}

inventory::submit! {
    SinkDescription::new_without_default::<HoneycombConfig>("honeycomb")
}

#[typetag::serde(name = "honeycomb")]
impl SinkConfig for HoneycombConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = self.batch.unwrap_or(bytesize::mib(5u64), 1);

        let sink = BatchedHttpSink::new(
            self.clone(),
            JsonArrayBuffer::default(),
            request_settings,
            batch_settings,
            None,
            &cx,
        )
        .sink_map_err(|e| error!("Fatal honeycomb sink error: {}", e));

        let healthcheck = Box::new(Box::pin(healthcheck(self.clone(), cx.resolver())).compat());

        Ok((Box::new(sink), healthcheck))
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

        Some(json!({
            "timestamp": timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "data": log.all_fields(),
        }))
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

async fn healthcheck(config: HoneycombConfig, resolver: Resolver) -> crate::Result<()> {
    let mut client = HttpClient::new(resolver, TlsSettings::from_options(&None)?)?;

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
