use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        http::{Auth, BatchedHttpSink, HttpClient, HttpSink},
        BatchConfig, BatchSettings, BoxedRawValue, JsonArrayBuffer, TowerRequestConfig, UriSerde,
    },
};
use futures::FutureExt;
use futures01::Sink;
use http::{Request, StatusCode, Uri};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::SystemTime;
use string_cache::DefaultAtom as Atom;

lazy_static::lazy_static! {
    static ref HOST: UriSerde = Uri::from_static("https://logs.logdna.com").into();
}

const PATH: &str = "/logs/ingest";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogdnaConfig {
    api_key: String,
    // Deprecated name
    #[serde(alias = "host")]
    endpoint: Option<UriSerde>,

    hostname: String,
    mac: Option<String>,
    ip: Option<String>,
    tags: Option<Vec<String>>,

    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,

    default_app: Option<String>,

    #[serde(default)]
    batch: BatchConfig,

    #[serde(default)]
    request: TowerRequestConfig,
}

inventory::submit! {
    SinkDescription::new::<LogdnaConfig>("logdna")
}

impl GenerateConfig for LogdnaConfig {}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

#[async_trait::async_trait]
#[typetag::serde(name = "logdna")]
impl SinkConfig for LogdnaConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = BatchSettings::default()
            .bytes(bytesize::mib(10u64))
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
        .sink_map_err(|e| error!("Fatal logdna sink error: {}", e));

        let healthcheck = healthcheck(self.clone(), client).boxed();

        Ok((
            super::VectorSink::Futures01Sink(Box::new(sink)),
            healthcheck,
        ))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "logdna"
    }
}

#[async_trait::async_trait]
impl HttpSink for LogdnaConfig {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.encoding.apply_rules(&mut event);
        let mut log = event.into_log();

        let line = log
            .remove(&Atom::from(crate::config::log_schema().message_key()))
            .unwrap_or_else(|| String::from("").into());
        let timestamp = log
            .remove(&Atom::from(crate::config::log_schema().timestamp_key()))
            .unwrap_or_else(|| chrono::Utc::now().into());

        let mut map = serde_json::map::Map::new();

        map.insert("line".to_string(), json!(line));
        map.insert("timestamp".to_string(), json!(timestamp));

        if let Some(app) = log.remove(&"app".into()) {
            map.insert("app".to_string(), json!(app));
        }

        if let Some(file) = log.remove(&"file".into()) {
            map.insert("file".to_string(), json!(file));
        }

        if !map.contains_key("app") && !map.contains_key("file") {
            if let Some(default_app) = &self.default_app {
                map.insert("app".to_string(), json!(default_app.as_str()));
            } else {
                map.insert("app".to_string(), json!("vector"));
            }
        }

        if !log.is_empty() {
            map.insert("meta".into(), json!(&log));
        }

        Some(map.into())
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let mut query = url::form_urlencoded::Serializer::new(String::new());

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Time can't drift behind the epoch!")
            .as_millis();

        query.append_pair("hostname", &self.hostname);
        query.append_pair("now", &format!("{}", now));

        if let Some(mac) = &self.mac {
            query.append_pair("mac", mac);
        }

        if let Some(ip) = &self.ip {
            query.append_pair("ip", ip);
        }

        if let Some(tags) = &self.tags {
            let tags = tags.join(",");
            query.append_pair("tags", &tags);
        }

        let query = query.finish();

        let body = serde_json::to_vec(&json!({
            "lines": events,
        }))
        .unwrap();

        let uri = self.build_uri(&query);

        let mut request = Request::builder()
            .uri(uri)
            .method("POST")
            .header("Content-Type", "application/json")
            .body(body)
            .unwrap();

        let auth = Auth::Basic {
            user: self.api_key.clone(),
            password: "".to_string(),
        };

        auth.apply(&mut request);

        Ok(request)
    }
}

impl LogdnaConfig {
    fn build_uri(&self, query: &str) -> Uri {
        let host: Uri = self.endpoint.clone().unwrap_or_else(|| HOST.clone()).into();

        let uri = format!("{}{}?{}", host, PATH, query);

        uri.parse::<http::Uri>()
            .expect("This should be a valid uri")
    }
}

async fn healthcheck(config: LogdnaConfig, mut client: HttpClient) -> crate::Result<()> {
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
    use super::*;
    use crate::{
        config::SinkConfig,
        event::Event,
        sinks::util::test::{build_test_server, load_sink},
        test_util::{next_addr, random_lines, trace_init},
    };
    use futures::{stream, StreamExt};
    use serde_json::json;

    #[test]
    fn encode_event() {
        let (config, _cx) = load_sink::<LogdnaConfig>(
            r#"
            api_key = "mylogtoken"
            hostname = "vector"
            codec.except_fields = ["magic"]
        "#,
        )
        .unwrap();

        let mut event1 = Event::from("hello world");
        event1.as_mut_log().insert("app", "notvector");
        event1.as_mut_log().insert("magic", "vector");

        let mut event2 = Event::from("hello world");
        event2.as_mut_log().insert("file", "log.txt");

        let event3 = Event::from("hello world");

        let event1_out = config.encode_event(event1).unwrap();
        let event1_out = event1_out.as_object().unwrap();
        let event2_out = config.encode_event(event2).unwrap();
        let event2_out = event2_out.as_object().unwrap();
        let event3_out = config.encode_event(event3).unwrap();
        let event3_out = event3_out.as_object().unwrap();

        assert_eq!(event1_out.get("app").unwrap(), &json!("notvector"));
        assert_eq!(event2_out.get("file").unwrap(), &json!("log.txt"));
        assert_eq!(event3_out.get("app").unwrap(), &json!("vector"));
    }

    #[tokio::test]
    async fn smoke() {
        trace_init();

        let (mut config, cx) = load_sink::<LogdnaConfig>(
            r#"
            api_key = "mylogtoken"
            ip = "127.0.0.1"
            mac = "some-mac-addr"
            hostname = "vector"
            tags = ["test","maybeanothertest"]
        "#,
        )
        .unwrap();

        // Make sure we can build the config
        let _ = config.build(cx.clone()).await.unwrap();

        let addr = next_addr();
        // Swap out the host so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr).parse::<http::Uri>().unwrap();
        config.endpoint = Some(endpoint.into());

        let (sink, _) = config.build(cx).await.unwrap();

        let (mut rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        let lines = random_lines(100).take(10).collect::<Vec<_>>();
        let mut events = Vec::new();

        // Create 10 events where the first one contains custom
        // fields that are not just `message`.
        for (i, line) in lines.iter().enumerate() {
            let event = if i == 0 {
                let mut event = Event::from(line.as_str());
                event.as_mut_log().insert("key1", "value1");
                event
            } else {
                Event::from(line.as_str())
            };

            events.push(event);
        }

        sink.run(stream::iter(events)).await.unwrap();

        let output = rx.next().await.unwrap();

        let request = &output.0;
        let body: serde_json::Value = serde_json::from_slice(&output.1[..]).unwrap();

        let query = request.uri.query().unwrap();
        assert!(query.contains("hostname=vector"));
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
            assert_eq!(line.get("line").unwrap(), &json!(lines[i]));

            if i == 0 {
                assert_eq!(
                    line.get("meta").unwrap(),
                    &json!({
                        "key1": "value1"
                    })
                );
            }
        }
    }
}
