use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    sinks::{
        util::{
            self,
            encoding::{EncodingConfig, EncodingConfiguration},
            http::{BatchedHttpSink, HttpClient, HttpSink},
            BatchConfig, BatchSettings, BoxedRawValue, Encoding, JsonArrayBuffer,
            TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::TlsConfig,
};
use bytes::Bytes;
use futures::FutureExt;
use futures01::Sink;
use http::{Request, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogLogsConfig {
    endpoint: Option<String>,
    api_key: String,
    encoding: EncodingConfig<Encoding>,
    tls: Option<TlsConfig>,

    #[serde(default)]
    batch: BatchConfig,

    #[serde(default)]
    request: TowerRequestConfig,
}

inventory::submit! {
    SinkDescription::new::<DatadogLogsConfig>("datadog_logs")
}

impl GenerateConfig for DatadogLogsConfig {}

impl DatadogLogsConfig {
    fn get_endpoint<'a>(&'a self) -> &'a str {
        match &self.endpoint {
            Some(ref endpoint) => &endpoint,
            None => "https://http-intake.logs.datadoghq.eu/v1/input",
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_logs")]
impl SinkConfig for DatadogLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = BatchSettings::default()
            .bytes(bytesize::kib(100u64))
            .timeout(1)
            .parse_config(self.batch)?;

        let client = HttpClient::new(cx.resolver(), None)?;

        let sink = BatchedHttpSink::new(
            self.clone(),
            JsonArrayBuffer::new(batch_settings.size),
            request_settings,
            batch_settings.timeout,
            client.clone(),
            cx.acker(),
        )
        .sink_map_err(|e| error!("Fatal datadog_logs sink error: {}", e));

        let healthcheck = healthcheck(self.clone(), client).boxed();

        Ok((VectorSink::Futures01Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "datadog_logs"
    }
}

#[async_trait::async_trait]
impl HttpSink for DatadogLogsConfig {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        let log = event.as_mut_log();

        if let Some(message) = log.remove(&Atom::from(log_schema().message_key())) {
            log.insert("message", message);
        }

        if let Some(timestamp) = log.remove(&Atom::from(log_schema().timestamp_key())) {
            log.insert("date", timestamp);
        }

        if let Some(host) = log.remove(&Atom::from(log_schema().host_key())) {
            log.insert("host", host);
        }

        self.encoding.apply_rules(&mut event);

        Some(json!(event.into_log()))
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let uri = self.get_endpoint();

        let request = Request::post(uri)
            .header("Content-Type", "application/json")
            .header("DD-API-KEY", self.api_key.clone());

        let buf = serde_json::to_vec(&events)?;
        request.body(buf).map_err(Into::into)
    }
}

async fn healthcheck(config: DatadogLogsConfig, mut client: HttpClient) -> crate::Result<()> {
    let req = config
        .build_request(Vec::new())
        .await?
        .map(hyper::Body::from);

    let res = client.send(req).await?;

    let status = res.status();
    let body = hyper::body::to_bytes(res.into_body()).await?;

    if status == StatusCode::OK {
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
