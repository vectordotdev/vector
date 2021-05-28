use super::healthcheck;
use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    http::HttpClient,
    internal_events::{DatadogEventsFieldInvalid, DatadogEventsProcessed},
    sinks::{
        util::{
            batch::Batch,
            encoding::{EncodingConfigWithDefault, EncodingConfiguration, TimestampFormat},
            http::{HttpSink, PartitionHttpSink},
            BatchConfig, BatchSettings, BoxedRawValue, EncodedEvent, JsonArrayBuffer,
            PartitionBuffer, PartitionInnerBuffer, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsConfig},
};
use futures::{FutureExt, SinkExt};
use http::Request;
use indoc::indoc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Json,
}

impl Default for Encoding {
    fn default() -> Self {
        Self::Json
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogEventsConfig {
    endpoint: Option<String>,

    #[serde(default = "default_site")]
    site: String,
    default_api_key: String,

    tls: Option<TlsConfig>,

    #[serde(default)]
    request: TowerRequestConfig,
}

type ApiKey = Arc<str>;

fn default_site() -> String {
    "datadoghq.com".to_owned()
}

inventory::submit! {
    SinkDescription::new::<DatadogEventsConfig>("datadog_events")
}

impl GenerateConfig for DatadogEventsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            default_api_key = "${DATADOG_API_KEY_ENV_VAR}"
        "#})
        .unwrap()
    }
}

impl DatadogEventsConfig {
    fn get_uri(&self) -> String {
        format!("{}/api/v1/events", self.get_api_endpoint())
    }

    fn get_api_endpoint(&self) -> String {
        self.endpoint
            .clone()
            .unwrap_or_else(|| format!("https://api.{}", &self.site))
    }

    fn build_sink<T, B, O>(
        &self,
        cx: SinkContext,
        service: T,
        batch: B,
        timeout: Duration,
    ) -> crate::Result<(VectorSink, Healthcheck)>
    where
        O: 'static,
        B: Batch<Output = Vec<O>> + std::marker::Send + 'static,
        B::Output: std::marker::Send + Clone,
        B::Input: std::marker::Send,
        T: HttpSink<
                Input = PartitionInnerBuffer<B::Input, ApiKey>,
                Output = PartitionInnerBuffer<B::Output, ApiKey>,
            > + Clone,
    {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());

        let tls_settings = MaybeTlsSettings::from_config(
            &Some(self.tls.clone().unwrap_or_else(TlsConfig::enabled)),
            false,
        )?;

        let client = HttpClient::new(tls_settings)?;
        let healthcheck = healthcheck(
            self.get_api_endpoint().clone(),
            self.default_api_key.clone(),
            client.clone(),
        )
        .boxed();
        let sink = PartitionHttpSink::new(
            service,
            PartitionBuffer::new(batch),
            request_settings,
            timeout,
            client,
            cx.acker(),
        )
        .sink_map_err(|error| error!(message = "Fatal datadog_metrics sink error.", %error));

        Ok((VectorSink::Sink(Box::new(sink)), healthcheck))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_events")]
impl SinkConfig for DatadogEventsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batch_settings = BatchSettings::default()
            .bytes(bytesize::kib(100u64))
            .events(1)
            .timeout(0)
            .parse_config(BatchConfig::default())?;

        self.build_sink(
            cx,
            DatadogEventsService::new(self),
            JsonArrayBuffer::new(batch_settings.size),
            batch_settings.timeout,
        )
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "datadog_events"
    }
}

#[derive(Clone)]
struct DatadogEventsService {
    config: DatadogEventsConfig,
    // Used to store the complete URI and avoid calling `get_uri` for each request
    uri: String,
    default_api_key: ApiKey,
    encoding: EncodingConfigWithDefault<()>,
}

impl DatadogEventsService {
    fn new(config: &DatadogEventsConfig) -> Self {
        let mut encoding = EncodingConfigWithDefault::default();

        // DataDog Event API allows only some fields, and refuses
        // to accept event if it contains any other field.
        encoding.only_fields = Some(vec![
            vec!["aggregation_key".into()],
            vec!["alert_type".into()],
            vec!["date_happened".into()],
            vec!["device_name".into()],
            vec!["host".into()],
            vec!["priority".into()],
            vec!["related_event_id".into()],
            vec!["source_type_name".into()],
            vec!["tags".into()],
            vec!["text".into()],
            vec!["title".into()],
        ]);

        // DataDog Event API requires unix timestamp.
        encoding.timestamp_format = Some(TimestampFormat::Unix);

        Self {
            default_api_key: Arc::from(config.default_api_key.clone()),

            uri: config.get_uri(),

            encoding,
            config: config.clone(),
        }
    }
}

#[async_trait::async_trait]
impl HttpSink for DatadogEventsService {
    type Input = PartitionInnerBuffer<serde_json::Value, ApiKey>;
    type Output = PartitionInnerBuffer<Vec<BoxedRawValue>, ApiKey>;

    fn encode_event(&self, mut event: Event) -> Option<EncodedEvent<Self::Input>> {
        let log = event.as_mut_log();

        if !log.contains("title") {
            emit!(DatadogEventsFieldInvalid { field: "title" });
            return None;
        }

        if !log.contains("text") {
            if let Some(message) = log.remove(log_schema().message_key()) {
                log.insert("text", message);
            } else {
                emit!(DatadogEventsFieldInvalid {
                    field: log_schema().message_key()
                });
                return None;
            }
        }

        if !log.contains("host") {
            if let Some(host) = log.remove(log_schema().host_key()) {
                log.insert("host", host);
            }
        }

        if !log.contains("date_happened") {
            if let Some(timestamp) = log.remove(log_schema().timestamp_key()) {
                log.insert("date_happened", timestamp);
            }
        }

        if !log.contains("source_type_name") {
            if let Some(name) = log.remove(log_schema().source_type_key()) {
                log.insert("source_type_name", name);
            }
        }

        self.encoding.apply_rules(&mut event);  

        let (fields, metadata) = event.into_log().into_parts();
        let json_event = json!(fields);
        let api_key = metadata
            .datadog_api_key()
            .as_ref()
            .unwrap_or(&self.default_api_key);

        Some(EncodedEvent {
            item: PartitionInnerBuffer::new(json_event, Arc::clone(api_key)),
            metadata: Some(metadata),
        })
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        let (mut events, api_key) = events.into_parts();

        let body = serde_json::to_vec(&events.pop().unwrap())?;

        emit!(DatadogEventsProcessed {
            byte_size: body.len(),
        });

        Request::post(self.uri.as_str())
            .header("Content-Type", "application/json")
            .header("DD-API-KEY", &api_key[..])
            .header("Content-Length", body.len())
            .body(body)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::SinkConfig,
        sinks::util::test::{build_test_server_status, load_sink},
        test_util::{next_addr, random_lines_with_stream},
    };
    use futures::{
        channel::mpsc::{Receiver, TryRecvError},
        stream, StreamExt,
    };
    use hyper::StatusCode;
    use indoc::indoc;
    use pretty_assertions::assert_eq;
    use vector_core::event::{BatchNotifier, BatchStatus};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogEventsConfig>();
    }

    fn event_with_api_key(msg: &str, key: &str) -> Event {
        let mut e = Event::from(msg);
        e.as_mut_log()
            .metadata_mut()
            .set_datadog_api_key(Some(Arc::from(key)));
        e
    }
    #[tokio::test]
    async fn smoke() {
        let (expected, rx) = start_test(StatusCode::OK, BatchStatus::Delivered).await;

        let output = rx.take(expected.len()).collect::<Vec<_>>().await;

        for (i, val) in output.iter().enumerate() {
            assert_eq!(
                val.0.headers.get("Content-Type").unwrap(),
                "application/json"
            );

            let mut json = serde_json::Deserializer::from_slice(&val.1[..])
                .into_iter::<serde_json::Value>()
                .map(|v| v.expect("decoding json"));

            let json = json.next().unwrap();

            // The json we send to Datadog is an array of events.
            // As we have set batch.max_events to 1, each entry will be
            // an array containing a single record.
            let message = json.get(0).unwrap().get("text").unwrap().as_str().unwrap();
            assert_eq!(message, expected[i]);
        }
    }

    #[tokio::test]
    async fn handles_failure() {
        let (_expected, mut rx) = start_test(StatusCode::FORBIDDEN, BatchStatus::Failed).await;

        assert!(matches!(rx.try_next(), Err(TryRecvError { .. })));
    }

    async fn start_test(
        http_status: StatusCode,
        batch_status: BatchStatus,
    ) -> (Vec<String>, Receiver<(http::request::Parts, Bytes)>) {
        let config = indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
            batch.max_events = 1
        "#};
        let (mut config, cx) = load_sink::<DatadogEventsConfig>(&config).unwrap();

        let addr = next_addr();
        // Swap out the endpoint so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr);
        config.endpoint = Some(endpoint.clone());

        let (sink, _) = config.build(cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server_status(addr, http_status);
        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (expected, events) = random_lines_with_stream(100, 10, Some(batch));

        let _ = sink.run(events).await.unwrap();

        assert_eq!(receiver.try_recv(), Ok(batch_status));

        (expected, rx)
    }

    #[tokio::test]
    async fn api_key_in_metadata() {
        let (mut config, cx) = load_sink::<DatadogEventsConfig>(indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
            batch.max_events = 1
        "#})
        .unwrap();

        let addr = next_addr();
        // Swap out the endpoint so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr);
        config.endpoint = Some(endpoint.clone());

        let (sink, _) = config.build(cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server_status(addr, StatusCode::OK);
        tokio::spawn(server);

        let (expected, events) = random_lines_with_stream(100, 10, None);

        let mut events = events.map(|mut e| {
            e.as_mut_log()
                .metadata_mut()
                .set_datadog_api_key(Some(Arc::from("from_metadata")));
            Ok(e)
        });

        let _ = sink.into_sink().send_all(&mut events).await.unwrap();
        let output = rx.take(expected.len()).collect::<Vec<_>>().await;

        for (i, val) in output.iter().enumerate() {
            assert_eq!(val.0.headers.get("DD-API-KEY").unwrap(), "from_metadata");

            assert_eq!(
                val.0.headers.get("Content-Type").unwrap(),
                "application/json"
            );

            let mut json = serde_json::Deserializer::from_slice(&val.1[..])
                .into_iter::<serde_json::Value>()
                .map(|v| v.expect("decoding json"));

            let json = json.next().unwrap();

            // The json we send to Datadog is an array of events.
            // As we have set batch.max_events to 1, each entry will be
            // an array containing a single record.
            let message = json.get(0).unwrap().get("text").unwrap().as_str().unwrap();
            assert_eq!(message, expected[i]);
        }
    }

    #[tokio::test]
    async fn multiple_api_keys() {
        let (mut config, cx) = load_sink::<DatadogEventsConfig>(indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
            batch.max_events = 1
        "#})
        .unwrap();

        let addr = next_addr();
        // Swap out the endpoint so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr);
        config.endpoint = Some(endpoint.clone());

        let (sink, _) = config.build(cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server_status(addr, StatusCode::OK);
        tokio::spawn(server);

        let events = vec![
            event_with_api_key("mow", "pkc"),
            event_with_api_key("pnh", "vvo"),
            Event::from("no API key in metadata"),
        ];

        let _ = sink.run(stream::iter(events)).await.unwrap();

        let mut keys = rx
            .take(3)
            .map(|r| r.0.headers.get("DD-API-KEY").unwrap().clone())
            .collect::<Vec<_>>()
            .await;

        keys.sort();
        assert_eq!(keys, vec!["atoken", "pkc", "vvo"])
    }
}
