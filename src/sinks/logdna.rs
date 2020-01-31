use crate::{
    event::{self, Event},
    sinks::util::http::{BatchedHttpSink, HttpSink},
    sinks::util::{
        Batch, BatchBytesConfig, BoxedRawValue, Compression, JsonArrayBuffer, TowerRequestConfig,
        UriSerde,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use http::{Request, Uri};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::SystemTime;

lazy_static::lazy_static! {
    static ref HOST: UriSerde = Uri::from_static("https://logs.logdna.com/logs/ingest").into();
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct LogdnaConfig {
    api_key: String,
    host: Option<UriSerde>,

    // Tags
    hostname: String,
    mac: Option<String>,
    ip: Option<String>,
    tags: Option<Vec<String>>,

    #[serde(default)]
    batch: BatchBytesConfig,

    #[serde(default)]
    request: TowerRequestConfig,
}

inventory::submit! {
    SinkDescription::new::<LogdnaConfig>("logdna")
}

#[typetag::serde(name = "logdna")]
impl SinkConfig for LogdnaConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = self.batch.unwrap_or(bytesize::mib(10u64), 1);

        let sink = BatchedHttpSink::new(
            self.clone(),
            JsonArrayBuffer::default(),
            request_settings,
            batch_settings,
            None,
            &cx,
        );

        let healthcheck = Box::new(futures::future::ok(()));

        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "logdna"
    }
}

impl HttpSink for LogdnaConfig {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
        let mut log = event.into_log();

        let line = log.remove(&event::MESSAGE).unwrap_or_else(|| "".into());
        let timestamp = log
            .remove(&event::TIMESTAMP)
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

        if !map.contains_key("app") || !map.contains_key("file") {
            map.insert("app".to_string(), json!("vector"));
        }

        let unflatten = log.unflatten();
        if !unflatten.is_empty() {
            map.insert("meta".to_string(), json!(unflatten));
        }

        Some(map.into())
    }

    fn build_request(&self, events: Self::Output) -> http::Request<Vec<u8>> {
        let mut query = url::form_urlencoded::Serializer::new(format!(""));

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

        let host: Uri = self.host.clone().unwrap_or_else(|| HOST.clone()).into();

        let uri = format!("{}{}", host, query);

        Request::builder()
            .uri(uri)
            .method("POST")
            .body(body)
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use crate::sinks::util::test::{build_test_server, load_sink};
    use crate::test_util;
    use crate::topology::config::SinkConfig;
    use futures::{Sink, Stream};

    #[test]
    fn smoke() {
        let (mut config, cx, mut rt) = load_sink::<LogdnaConfig>(
            r#"
            api_key = "mylogtoken"
            hostname = "vector"
        "#,
        )
        .unwrap();

        // Make sure we can build the config
        let _ = config.build(cx.clone()).unwrap();

        let addr = test_util::next_addr();
        // Swap out the host so we can force send it
        // to our local server
        let host = format!("http://{}", addr).parse::<http::Uri>().unwrap();
        config.host = Some(host.into());

        let (sink, _) = config.build(cx).unwrap();

        let (rx, _trigger, server) = build_test_server(&addr);
        rt.spawn(server);

        let (expected, lines) = test_util::random_lines_with_stream(100, 10);
        let pump = sink.send_all(lines.map(Event::from));
        let _ = rt.block_on(pump).unwrap();

        let output = rx.take(1).wait().collect::<Result<Vec<_>, _>>().unwrap();

        let json: serde_json::Value = serde_json::from_slice(&output[0].1[..]).unwrap();

        // TODO: write assertions
        println!("json: {:?}", json);
    }
}
