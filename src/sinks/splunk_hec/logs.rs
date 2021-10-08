use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::{Event, LogEvent, Value},
    internal_events::{SplunkEventEncodeError, SplunkEventSent},
    sinks::splunk_hec::conn,
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        http::HttpSink,
        BatchConfig, Compression, TowerRequestConfig,
    },
    sinks::{Healthcheck, VectorSink},
    template::Template,
    tls::TlsOptions,
};
use http::Request;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct HecSinkLogsConfig {
    pub token: String,
    // Deprecated name
    #[serde(alias = "host")]
    pub endpoint: String,
    #[serde(default = "host_key")]
    pub host_key: String,
    #[serde(default)]
    pub indexed_fields: Vec<String>,
    pub index: Option<Template>,
    pub sourcetype: Option<Template>,
    pub source: Option<Template>,
    pub encoding: EncodingConfig<Encoding>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, Derivative)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

fn host_key() -> String {
    crate::config::log_schema().host_key().to_string()
}

inventory::submit! {
    SinkDescription::new::<HecSinkLogsConfig>("splunk_hec")
}

inventory::submit! {
    SinkDescription::new::<HecSinkLogsConfig>("splunk_hec_logs")
}

impl GenerateConfig for HecSinkLogsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            token: "${VECTOR_SPLUNK_HEC_TOKEN}".to_owned(),
            endpoint: "endpoint".to_owned(),
            host_key: host_key(),
            indexed_fields: vec![],
            index: None,
            sourcetype: None,
            source: None,
            encoding: Encoding::Text.into(),
            compression: Compression::default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec_logs")]
impl SinkConfig for HecSinkLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        conn::build_sink(
            self.clone(),
            &self.request,
            &self.tls,
            cx.proxy(),
            self.batch,
            self.compression,
            cx.acker(),
            &self.endpoint,
            &self.token,
        )
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "splunk_hec_logs"
    }
}

// Add a compatibility alias to avoid breaking existing configs
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct HecSinkCompatConfig {
    #[serde(flatten)]
    config: HecSinkLogsConfig,
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec")]
impl SinkConfig for HecSinkCompatConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        self.config.build(cx).await
    }

    fn input_type(&self) -> DataType {
        self.config.input_type()
    }

    fn sink_type(&self) -> &'static str {
        "splunk_hec"
    }
}

#[async_trait::async_trait]
impl HttpSink for HecSinkLogsConfig {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
        let sourcetype = self
            .sourcetype
            .as_ref()
            .and_then(|sourcetype| super::render_template_string(sourcetype, &event, "sourcetype"));

        let source = self
            .source
            .as_ref()
            .and_then(|source| super::render_template_string(source, &event, "source"));

        let index = self
            .index
            .as_ref()
            .and_then(|index| super::render_template_string(index, &event, "index"));

        let mut event = event.into_log();

        let host = event.get(self.host_key.to_owned()).cloned();

        let timestamp = match event.remove(log_schema().timestamp_key()) {
            Some(Value::Timestamp(ts)) => ts,
            _ => chrono::Utc::now(),
        };
        let timestamp = (timestamp.timestamp_millis() as f64) / 1000f64;

        let fields = self
            .indexed_fields
            .iter()
            .filter_map(|field| event.get(field).map(|value| (field, value.clone())))
            .collect::<LogEvent>();

        let mut event = Event::Log(event);
        self.encoding.apply_rules(&mut event);
        let log = event.into_log();

        let event = match self.encoding.codec() {
            Encoding::Json => json!(&log),
            Encoding::Text => json!(log
                .get(log_schema().message_key())
                .map(|v| v.to_string_lossy())
                .unwrap_or_else(|| "".into())),
        };

        let mut body = json!({
            "event": event,
            "fields": fields,
            "time": timestamp
        });

        if let Some(host) = host {
            let host = host.to_string_lossy();
            body["host"] = json!(host);
        }

        if let Some(index) = index {
            body["index"] = json!(index);
        }

        if let Some(source) = source {
            body["source"] = json!(source);
        }

        if let Some(sourcetype) = &sourcetype {
            body["sourcetype"] = json!(sourcetype);
        }

        match serde_json::to_vec(&body) {
            Ok(value) => {
                emit!(&SplunkEventSent {
                    byte_size: value.len()
                });
                Some(value)
            }
            Err(error) => {
                emit!(&SplunkEventEncodeError { error });
                None
            }
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        conn::build_request(&self.endpoint, &self.token, self.compression, events).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use crate::sinks::util::{http::HttpSink, test::load_sink};
    use chrono::Utc;
    use serde::Deserialize;
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HecSinkLogsConfig>();
    }

    #[derive(Deserialize, Debug)]
    struct HecEventJson {
        time: f64,
        event: BTreeMap<String, String>,
        fields: BTreeMap<String, String>,
        source: Option<String>,
    }

    #[derive(Deserialize, Debug)]
    struct HecEventText {
        time: f64,
        event: String,
        fields: BTreeMap<String, String>,
    }

    #[test]
    fn splunk_encode_log_event_json() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("key", "value");
        event.as_mut_log().insert("magic", "vector");

        let (config, _cx) = load_sink::<HecSinkLogsConfig>(
            r#"
            host = "test.com"
            token = "alksjdfo"
            host_key = "host"
            indexed_fields = ["key"]
            source = "{{ magic }}"

            [encoding]
            codec = "json"
            except_fields = ["magic"]
        "#,
        )
        .unwrap();

        let bytes = config.encode_event(event).unwrap();

        let hec_event = serde_json::from_slice::<HecEventJson>(&bytes[..]).unwrap();

        let event = &hec_event.event;
        let kv = event.get(&"key".to_string()).unwrap();

        assert_eq!(kv, &"value".to_string());
        assert_eq!(
            event[&log_schema().message_key().to_string()],
            "hello world".to_string()
        );
        assert!(event
            .get(&log_schema().timestamp_key().to_string())
            .is_none());

        assert!(!event.contains_key("magic"));
        assert_eq!(hec_event.source, Some("vector".to_string()));

        assert_eq!(
            hec_event.fields.get("key").map(|s| s.as_str()),
            Some("value")
        );

        let now = Utc::now().timestamp_millis() as f64 / 1000f64;
        assert!(
            (hec_event.time - now).abs() < 0.2,
            "hec_event.time = {}, now = {}",
            hec_event.time,
            now
        );
        assert_eq!((hec_event.time * 1000f64).fract(), 0f64);
    }

    #[test]
    fn splunk_encode_log_event_text() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("key", "value");

        let (config, _cx) = load_sink::<HecSinkLogsConfig>(
            r#"
            host = "test.com"
            token = "alksjdfo"
            host_key = "host"
            indexed_fields = ["key"]

            [encoding]
            codec = "text"
        "#,
        )
        .unwrap();

        let bytes = config.encode_event(event).unwrap();

        let hec_event = serde_json::from_slice::<HecEventText>(&bytes[..]).unwrap();

        assert_eq!(hec_event.event.as_str(), "hello world");

        assert_eq!(
            hec_event.fields.get("key").map(|s| s.as_str()),
            Some("value")
        );

        let now = Utc::now().timestamp_millis() as f64 / 1000f64;
        assert!(
            (hec_event.time - now).abs() < 0.2,
            "hec_event.time = {}, now = {}",
            hec_event.time,
            now
        );
        assert_eq!((hec_event.time * 1000f64).fract(), 0f64);
    }
}

#[cfg(all(test, feature = "splunk-integration-tests"))]
mod integration_tests {
    use super::*;
    use crate::{
        config::{SinkConfig, SinkContext},
        sinks::splunk_hec::conn::integration_test_helpers::get_token,
        test_util::{components, random_lines_with_stream, random_string},
    };
    use futures::stream;
    use serde_json::Value as JsonValue;
    use std::convert::TryFrom;
    use std::future::ready;
    use tokio::time::{sleep, Duration};
    use vector_core::event::{BatchNotifier, BatchStatus, Event, LogEvent};

    const USERNAME: &str = "admin";
    const PASSWORD: &str = "password";

    const SINK_TAGS: [&str; 1] = ["endpoint"];

    // It usually takes ~1 second for the event to show up in search, so poll until
    // we see it.
    async fn find_entry(message: &str) -> serde_json::value::Value {
        for _ in 0..20usize {
            match recent_entries(None)
                .await
                .into_iter()
                .find(|entry| entry["_raw"].as_str().unwrap_or("").contains(&message))
            {
                Some(value) => return value,
                None => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
        panic!("Didn't find event in Splunk");
    }

    #[tokio::test]
    async fn splunk_insert_message() {
        let cx = SinkContext::new_test();

        let config = config(Encoding::Text, vec![]).await;
        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let event = LogEvent::from(message.clone())
            .with_batch_notifier(&batch)
            .into();
        drop(batch);
        components::run_sink_event(sink, event, &SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let entry = find_entry(message.as_str()).await;

        assert_eq!(message, entry["_raw"].as_str().unwrap());
        assert!(entry.get("message").is_none());
    }

    #[tokio::test]
    async fn splunk_insert_broken_token() {
        let cx = SinkContext::new_test();

        let mut config = config(Encoding::Text, vec![]).await;
        config.token = "BROKEN_TOKEN".into();
        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let event = LogEvent::from(message.clone())
            .with_batch_notifier(&batch)
            .into();
        drop(batch);
        sink.run(stream::once(ready(event))).await.unwrap();
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Failed));
    }

    #[tokio::test]
    async fn splunk_insert_source() {
        let cx = SinkContext::new_test();

        let mut config = config(Encoding::Text, vec![]).await;
        config.source = Template::try_from("/var/log/syslog".to_string()).ok();

        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let event = Event::from(message.clone());
        components::run_sink_event(sink, event, &SINK_TAGS).await;

        let entry = find_entry(message.as_str()).await;

        assert_eq!(entry["source"].as_str(), Some("/var/log/syslog"));
    }

    #[tokio::test]
    async fn splunk_insert_index() {
        let cx = SinkContext::new_test();

        let mut config = config(Encoding::Text, vec![]).await;
        config.index = Template::try_from("custom_index".to_string()).ok();
        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let event = Event::from(message.clone());
        components::run_sink_event(sink, event, &SINK_TAGS).await;

        let entry = find_entry(message.as_str()).await;

        assert_eq!(entry["index"].as_str().unwrap(), "custom_index");
    }

    #[tokio::test]
    async fn splunk_index_is_interpolated() {
        let cx = SinkContext::new_test();

        let indexed_fields = vec!["asdf".to_string()];
        let mut config = config(Encoding::Json, indexed_fields).await;
        config.index = Template::try_from("{{ index_name }}".to_string()).ok();

        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("index_name", "custom_index");
        components::run_sink_event(sink, event, &SINK_TAGS).await;

        let entry = find_entry(message.as_str()).await;

        let index = entry["index"].as_str().unwrap();
        assert_eq!("custom_index", index);
    }

    #[tokio::test]
    async fn splunk_insert_many() {
        let cx = SinkContext::new_test();

        let config = config(Encoding::Text, vec![]).await;
        let (sink, _) = config.build(cx).await.unwrap();

        let (messages, events) = random_lines_with_stream(100, 10, None);
        components::run_sink(sink, events, &SINK_TAGS).await;

        let mut found_all = false;
        for _ in 0..20 {
            let entries = recent_entries(None).await;

            found_all = messages.iter().all(|message| {
                entries
                    .iter()
                    .any(|entry| entry["_raw"].as_str().unwrap() == message)
            });

            if found_all {
                break;
            }

            sleep(Duration::from_millis(100)).await;
        }

        assert!(found_all);
    }

    #[tokio::test]
    async fn splunk_custom_fields() {
        let cx = SinkContext::new_test();

        let indexed_fields = vec!["asdf".into()];
        let config = config(Encoding::Json, indexed_fields).await;
        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("asdf", "hello");
        components::run_sink_event(sink, event, &SINK_TAGS).await;

        let entry = find_entry(message.as_str()).await;

        assert_eq!(message, entry["message"].as_str().unwrap());
        let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("hello", asdf);
    }

    #[tokio::test]
    async fn splunk_hostname() {
        let cx = SinkContext::new_test();

        let indexed_fields = vec!["asdf".into()];
        let config = config(Encoding::Json, indexed_fields).await;
        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("asdf", "hello");
        event.as_mut_log().insert("host", "example.com:1234");
        components::run_sink_event(sink, event, &SINK_TAGS).await;

        let entry = find_entry(message.as_str()).await;

        assert_eq!(message, entry["message"].as_str().unwrap());
        let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("hello", asdf);
        let host = entry["host"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("example.com:1234", host);
    }

    #[tokio::test]
    async fn splunk_sourcetype() {
        let cx = SinkContext::new_test();

        let indexed_fields = vec!["asdf".to_string()];
        let mut config = config(Encoding::Json, indexed_fields).await;
        config.sourcetype = Template::try_from("_json".to_string()).ok();

        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("asdf", "hello");
        components::run_sink_event(sink, event, &SINK_TAGS).await;

        let entry = find_entry(message.as_str()).await;

        assert_eq!(message, entry["message"].as_str().unwrap());
        let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("hello", asdf);
        let sourcetype = entry["sourcetype"].as_str().unwrap();
        assert_eq!("_json", sourcetype);
    }

    #[tokio::test]
    async fn splunk_configure_hostname() {
        let cx = SinkContext::new_test();

        let config = HecSinkLogsConfig {
            host_key: "roast".into(),
            ..config(Encoding::Json, vec!["asdf".to_string()]).await
        };

        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("asdf", "hello");
        event.as_mut_log().insert("host", "example.com:1234");
        event.as_mut_log().insert("roast", "beef.example.com:1234");
        components::run_sink_event(sink, event, &SINK_TAGS).await;

        let entry = find_entry(message.as_str()).await;

        assert_eq!(message, entry["message"].as_str().unwrap());
        let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("hello", asdf);
        let host = entry["host"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("beef.example.com:1234", host);
    }

    async fn recent_entries(index: Option<&str>) -> Vec<JsonValue> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        // https://docs.splunk.com/Documentation/Splunk/7.2.1/RESTREF/RESTsearch#search.2Fjobs
        let search_query = match index {
            Some(index) => format!("search index={}", index),
            None => "search *".into(),
        };
        let res = client
            .post("https://localhost:8089/services/search/jobs?output_mode=json")
            .form(&vec![
                ("search", &search_query[..]),
                ("exec_mode", "oneshot"),
                ("f", "*"),
            ])
            .basic_auth(USERNAME, Some(PASSWORD))
            .send()
            .await
            .unwrap();
        let json: JsonValue = res.json().await.unwrap();

        json["results"].as_array().unwrap().clone()
    }

    async fn config(
        encoding: impl Into<EncodingConfig<Encoding>>,
        indexed_fields: Vec<String>,
    ) -> HecSinkLogsConfig {
        HecSinkLogsConfig {
            token: get_token().await,
            endpoint: "http://localhost:8088/".into(),
            host_key: "host".into(),
            indexed_fields,
            index: None,
            sourcetype: None,
            source: None,
            encoding: encoding.into(),
            compression: Compression::None,
            batch: BatchConfig {
                max_events: Some(1),
                ..Default::default()
            },
            request: TowerRequestConfig::default(),
            tls: None,
        }
    }
}
