use crate::{
    event::{self, Event},
    sinks::util::http::HttpService,
    sinks::util::{BatchBytesConfig, Compression, TowerRequestConfig},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use http::{Request, Uri};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct LogdnaConfig {
    api_key: String,
    host: Option<String>,

    // Tags
    hostname: String,
    mac: Option<String>,
    ip: Option<String>,

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

        let host = self
            .host
            .unwrap_or("https://logs.logdna.com/logs/ingest".to_string());

        let uri = Uri::from_shared(host.into())?;
        let config = Arc::new(*self.clone());

        let build_request = move |body| build_request(config.clone(), uri.clone(), body);

        let sink = HttpService::with_batched_encoded(
            cx,
            &request_settings,
            &batch_settings,
            build_request,
            encode_event,
        );
        let healthcheck = Box::new(futures::future::ok(()));

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "logdna"
    }
}

fn build_request(config: Arc<LogdnaConfig>, host: Uri, body: Vec<u8>) -> http::Request<Vec<u8>> {
    let body = String::from_utf8(body).expect("unable to convert serde_json to string");
    let raw_value = RawValue::from_string(body);

    let body = serde_json::to_vec(json!({
        "line": raw_value,
    }))
    .unwrap();

    Request::builder()
        .uri(host)
        .method("POST")
        .body(body)
        .unwrap()
}

fn encode_event(event: Event) -> Option<Vec<u8>> {
    let mut log = event.into_log();

    let line = log.remove(&event::MESSAGE).unwrap_or_else(|| "".into());
    let app = log.get(&"app".into()).unwrap_or_else(|| todo!(""));
    // let level = event
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

    serde_json::to_vec(&map)
        .map_err(|e| panic!("Unable to encode into JSON: {}", e))
        .ok()
}
