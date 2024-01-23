use std::time::SystemTime;

use bytes::Bytes;
use futures::{FutureExt, SinkExt};
use http::{Request, StatusCode, Uri};
use serde_json::json;
use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;
use vrl::event_path;
use vrl::value::{Kind, Value};

use crate::{
    codecs::Transformer,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    event::Event,
    http::{Auth, HttpClient},
    schema,
    sinks::util::{
        http::{HttpEventEncoder, HttpSink, PartitionHttpSink},
        BatchConfig, BoxedRawValue, JsonArrayBuffer, PartitionBuffer, PartitionInnerBuffer,
        RealtimeSizeBasedDefaultBatchSettings, TowerRequestConfig, UriSerde,
    },
    template::{Template, TemplateRenderingError},
};

const PATH: &str = "/logs/ingest";

/// Configuration for the `logdna` sink.
#[configurable_component(sink("logdna", "Deliver log event data to LogDNA."))]
#[configurable(metadata(
    deprecated = "The `logdna` sink has been renamed. Please use `mezmo` instead."
))]
#[derive(Clone, Debug)]
pub struct LogdnaConfig(MezmoConfig);

impl GenerateConfig for LogdnaConfig {
    fn generate_config() -> toml::Value {
        <MezmoConfig as GenerateConfig>::generate_config()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "logdna")]
impl SinkConfig for LogdnaConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        warn!("DEPRECATED: The `logdna` sink has been renamed. Please use `mezmo` instead.");
        self.0.build(cx).await
    }

    fn input(&self) -> Input {
        self.0.input()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        self.0.acknowledgements()
    }
}

/// Configuration for the `mezmo` (formerly `logdna`) sink.
#[configurable_component(sink("mezmo", "Deliver log event data to Mezmo."))]
#[derive(Clone, Debug)]
pub struct MezmoConfig {
    /// The Ingestion API key.
    #[configurable(metadata(docs::examples = "${LOGDNA_API_KEY}"))]
    #[configurable(metadata(docs::examples = "ef8d5de700e7989468166c40fc8a0ccd"))]
    api_key: SensitiveString,

    /// The HTTP endpoint to send logs to.
    ///
    /// Both IP address and hostname are accepted formats.
    #[serde(alias = "host")]
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(docs::examples = "http://127.0.0.1"))]
    #[configurable(metadata(docs::examples = "http://example.com"))]
    endpoint: UriSerde,

    /// The hostname that is attached to each batch of events.
    #[configurable(metadata(docs::examples = "${HOSTNAME}"))]
    #[configurable(metadata(docs::examples = "my-local-machine"))]
    hostname: Template,

    /// The MAC address that is attached to each batch of events.
    #[configurable(metadata(docs::examples = "my-mac-address"))]
    #[configurable(metadata(docs::human_name = "MAC Address"))]
    mac: Option<String>,

    /// The IP address that is attached to each batch of events.
    #[configurable(metadata(docs::examples = "0.0.0.0"))]
    #[configurable(metadata(docs::human_name = "IP Address"))]
    ip: Option<String>,

    /// The tags that are attached to each batch of events.
    #[configurable(metadata(docs::examples = "tag1"))]
    #[configurable(metadata(docs::examples = "tag2"))]
    tags: Option<Vec<Template>>,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    /// The default app that is set for events that do not contain a `file` or `app` field.
    #[serde(default = "default_app")]
    #[configurable(metadata(docs::examples = "my-app"))]
    default_app: String,

    /// The default environment that is set for events that do not contain an `env` field.
    #[serde(default = "default_env")]
    #[configurable(metadata(docs::examples = "staging"))]
    default_env: String,

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
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn default_endpoint() -> UriSerde {
    UriSerde {
        uri: Uri::from_static("https://logs.mezmo.com"),
        auth: None,
    }
}

fn default_app() -> String {
    "vector".to_owned()
}

fn default_env() -> String {
    "production".to_owned()
}

impl GenerateConfig for MezmoConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"hostname = "hostname"
            api_key = "${LOGDNA_API_KEY}""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "mezmo")]
impl SinkConfig for MezmoConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let request_settings = self.request.into_settings();
        let batch_settings = self.batch.into_batch_settings()?;
        let client = HttpClient::new(None, cx.proxy())?;

        let sink = PartitionHttpSink::new(
            self.clone(),
            PartitionBuffer::new(JsonArrayBuffer::new(batch_settings.size)),
            request_settings,
            batch_settings.timeout,
            client.clone(),
        )
        .sink_map_err(|error| error!(message = "Fatal mezmo sink error.", %error));

        let healthcheck = healthcheck(self.clone(), client).boxed();

        #[allow(deprecated)]
        Ok((super::VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        let requirement = schema::Requirement::empty()
            .optional_meaning("timestamp", Kind::timestamp())
            .optional_meaning("message", Kind::bytes());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub struct PartitionKey {
    hostname: String,
    tags: Option<Vec<String>>,
}

pub struct MezmoEventEncoder {
    hostname: Template,
    tags: Option<Vec<Template>>,
    transformer: Transformer,
    default_app: String,
    default_env: String,
}

impl MezmoEventEncoder {
    fn render_key(
        &self,
        event: &Event,
    ) -> Result<PartitionKey, (Option<&str>, TemplateRenderingError)> {
        let hostname = self
            .hostname
            .render_string(event)
            .map_err(|e| (Some("hostname"), e))?;
        let tags = self
            .tags
            .as_ref()
            .map(|tags| {
                let mut vec = Vec::with_capacity(tags.len());
                for tag in tags {
                    vec.push(tag.render_string(event).map_err(|e| (None, e))?);
                }
                Ok(Some(vec))
            })
            .unwrap_or(Ok(None))?;
        Ok(PartitionKey { hostname, tags })
    }
}

impl HttpEventEncoder<PartitionInnerBuffer<serde_json::Value, PartitionKey>> for MezmoEventEncoder {
    fn encode_event(
        &mut self,
        mut event: Event,
    ) -> Option<PartitionInnerBuffer<serde_json::Value, PartitionKey>> {
        let key = self
            .render_key(&event)
            .map_err(|(field, error)| {
                emit!(crate::internal_events::TemplateRenderingError {
                    error,
                    field,
                    drop_event: true,
                });
            })
            .ok()?;

        self.transformer.transform(&mut event);
        let mut log = event.into_log();

        let line = log
            .message_path()
            .cloned()
            .as_ref()
            .and_then(|path| log.remove(path))
            .unwrap_or_else(|| String::from("").into());

        let timestamp: Value = log
            .timestamp_path()
            .cloned()
            .and_then(|path| log.remove(&path))
            .unwrap_or_else(|| chrono::Utc::now().into());

        let mut map = serde_json::map::Map::new();

        map.insert("line".to_string(), json!(line));
        map.insert("timestamp".to_string(), json!(timestamp));

        if let Some(env) = log.remove(event_path!("env")) {
            map.insert("env".to_string(), json!(env));
        }

        if let Some(app) = log.remove(event_path!("app")) {
            map.insert("app".to_string(), json!(app));
        }

        if let Some(file) = log.remove(event_path!("file")) {
            map.insert("file".to_string(), json!(file));
        }

        if !map.contains_key("env") {
            map.insert("env".to_string(), json!(self.default_env));
        }

        if !map.contains_key("app") && !map.contains_key("file") {
            map.insert("app".to_string(), json!(self.default_app.as_str()));
        }

        if !log.is_empty_object() {
            map.insert("meta".into(), json!(&log));
        }

        Some(PartitionInnerBuffer::new(map.into(), key))
    }
}

#[async_trait::async_trait]
impl HttpSink for MezmoConfig {
    type Input = PartitionInnerBuffer<serde_json::Value, PartitionKey>;
    type Output = PartitionInnerBuffer<Vec<BoxedRawValue>, PartitionKey>;
    type Encoder = MezmoEventEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        MezmoEventEncoder {
            hostname: self.hostname.clone(),
            tags: self.tags.clone(),
            transformer: self.encoding.clone(),
            default_app: self.default_app.clone(),
            default_env: self.default_env.clone(),
        }
    }

    async fn build_request(&self, output: Self::Output) -> crate::Result<http::Request<Bytes>> {
        let (events, key) = output.into_parts();
        let mut query = url::form_urlencoded::Serializer::new(String::new());

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Time can't drift behind the epoch!")
            .as_millis();

        query.append_pair("hostname", &key.hostname);
        query.append_pair("now", &now.to_string());

        if let Some(mac) = &self.mac {
            query.append_pair("mac", mac);
        }

        if let Some(ip) = &self.ip {
            query.append_pair("ip", ip);
        }

        if let Some(tags) = &key.tags {
            let tags = tags.join(",");
            query.append_pair("tags", &tags);
        }

        let query = query.finish();

        let body = crate::serde::json::to_bytes(&json!({
            "lines": events,
        }))
        .unwrap()
        .freeze();

        let uri = self.build_uri(&query);

        let mut request = Request::builder()
            .uri(uri)
            .method("POST")
            .header("Content-Type", "application/json")
            .body(body)
            .unwrap();

        let auth = Auth::Basic {
            user: self.api_key.inner().to_string(),
            password: SensitiveString::default(),
        };

        auth.apply(&mut request);

        Ok(request)
    }
}

impl MezmoConfig {
    fn build_uri(&self, query: &str) -> Uri {
        let host = &self.endpoint.uri;

        let uri = format!("{}{}?{}", host, PATH, query);

        uri.parse::<http::Uri>()
            .expect("This should be a valid uri")
    }
}

async fn healthcheck(config: MezmoConfig, client: HttpClient) -> crate::Result<()> {
    let uri = config.build_uri("");

    let req = Request::post(uri).body(hyper::Body::empty()).unwrap();

    let res = client.send(req).await?;

    if res.status().is_server_error() {
        return Err("Server returned a server error".into());
    }

    if res.status() == StatusCode::FORBIDDEN {
        return Err("Token is not valid, 403 returned.".into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use futures::{channel::mpsc, StreamExt};
    use futures_util::stream;
    use http::{request::Parts, StatusCode};
    use serde_json::json;
    use vector_lib::event::{BatchNotifier, BatchStatus, Event, LogEvent};

    use super::*;
    use crate::{
        config::SinkConfig,
        sinks::util::test::{build_test_server_status, load_sink},
        test_util::{
            components::{assert_sink_compliance, HTTP_SINK_TAGS},
            next_addr, random_lines,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MezmoConfig>();
    }

    #[test]
    fn encode_event() {
        let (config, _cx) = load_sink::<MezmoConfig>(
            r#"
            api_key = "mylogtoken"
            hostname = "vector"
            default_env = "acceptance"
            codec.except_fields = ["magic"]
        "#,
        )
        .unwrap();
        let mut encoder = config.build_encoder();

        let mut event1 = Event::Log(LogEvent::from("hello world"));
        event1.as_mut_log().insert("app", "notvector");
        event1.as_mut_log().insert("magic", "vector");

        let mut event2 = Event::Log(LogEvent::from("hello world"));
        event2.as_mut_log().insert("file", "log.txt");

        let event3 = Event::Log(LogEvent::from("hello world"));

        let mut event4 = Event::Log(LogEvent::from("hello world"));
        event4.as_mut_log().insert("env", "staging");

        let event1_out = encoder.encode_event(event1).unwrap().into_parts().0;
        let event1_out = event1_out.as_object().unwrap();
        let event2_out = encoder.encode_event(event2).unwrap().into_parts().0;
        let event2_out = event2_out.as_object().unwrap();
        let event3_out = encoder.encode_event(event3).unwrap().into_parts().0;
        let event3_out = event3_out.as_object().unwrap();
        let event4_out = encoder.encode_event(event4).unwrap().into_parts().0;
        let event4_out = event4_out.as_object().unwrap();

        assert_eq!(event1_out.get("app").unwrap(), &json!("notvector"));
        assert_eq!(event2_out.get("file").unwrap(), &json!("log.txt"));
        assert_eq!(event3_out.get("app").unwrap(), &json!("vector"));
        assert_eq!(event3_out.get("env").unwrap(), &json!("acceptance"));
        assert_eq!(event4_out.get("env").unwrap(), &json!("staging"));
    }

    async fn smoke_start(
        status_code: StatusCode,
        batch_status: BatchStatus,
    ) -> (
        Vec<&'static str>,
        Vec<Vec<String>>,
        mpsc::Receiver<(Parts, bytes::Bytes)>,
    ) {
        let (mut config, cx) = load_sink::<MezmoConfig>(
            r#"
            api_key = "mylogtoken"
            ip = "127.0.0.1"
            mac = "some-mac-addr"
            hostname = "{{ hostname }}"
            tags = ["test","maybeanothertest"]
        "#,
        )
        .unwrap();

        // Make sure we can build the config
        _ = config.build(cx.clone()).await.unwrap();

        let addr = next_addr();
        // Swap out the host so we can force send it
        // to our local server
        let endpoint = UriSerde {
            uri: format!("http://{}", addr).parse::<http::Uri>().unwrap(),
            auth: None,
        };
        config.endpoint = endpoint;

        let (sink, _) = config.build(cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server_status(addr, status_code);
        tokio::spawn(server);

        let lines = random_lines(100).take(10).collect::<Vec<_>>();
        let mut events = Vec::new();
        let hosts = vec!["host0", "host1"];

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let mut partitions = vec![Vec::new(), Vec::new()];
        // Create 10 events where the first one contains custom
        // fields that are not just `message`.
        for (i, line) in lines.iter().enumerate() {
            let mut event = LogEvent::from(line.as_str()).with_batch_notifier(&batch);
            let p = i % 2;
            event.insert("hostname", hosts[p]);

            partitions[p].push(line.into());
            events.push(Event::Log(event));
        }
        drop(batch);

        let events = stream::iter(events).map(Into::into);
        sink.run(events).await.expect("Running sink failed");

        assert_eq!(receiver.try_recv(), Ok(batch_status));

        (hosts, partitions, rx)
    }

    #[tokio::test]
    async fn smoke_fails() {
        let (_hosts, _partitions, mut rx) =
            smoke_start(StatusCode::FORBIDDEN, BatchStatus::Rejected).await;
        assert!(matches!(rx.try_next(), Err(mpsc::TryRecvError { .. })));
    }

    #[tokio::test]
    async fn smoke() {
        assert_sink_compliance(&HTTP_SINK_TAGS, async {
            let (hosts, partitions, mut rx) =
                smoke_start(StatusCode::OK, BatchStatus::Delivered).await;

            for _ in 0..partitions.len() {
                let output = rx.next().await.unwrap();

                let request = &output.0;
                let body: serde_json::Value = serde_json::from_slice(&output.1[..]).unwrap();

                let query = request.uri.query().unwrap();

                let (p, host) = hosts
                    .iter()
                    .enumerate()
                    .find(|(_, host)| query.contains(&format!("hostname={}", host)))
                    .expect("invalid hostname");
                let lines = &partitions[p];

                assert!(query.contains("ip=127.0.0.1"));
                assert!(query.contains("mac=some-mac-addr"));
                assert!(query.contains("tags=test%2Cmaybeanothertest"));

                let output = body
                    .as_object()
                    .unwrap()
                    .get("lines")
                    .unwrap()
                    .as_array()
                    .unwrap();

                for (i, line) in output.iter().enumerate() {
                    // All lines are json objects
                    let line = line.as_object().unwrap();

                    assert_eq!(line.get("app").unwrap(), &json!("vector"));
                    assert_eq!(line.get("env").unwrap(), &json!("production"));
                    assert_eq!(line.get("line").unwrap(), &json!(lines[i]));

                    assert_eq!(
                        line.get("meta").unwrap(),
                        &json!({
                            "hostname": host,
                        })
                    );
                }
            }
        })
        .await;
    }
}
