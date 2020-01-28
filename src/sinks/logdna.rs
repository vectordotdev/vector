use crate::{
    event::{self, Event},
    sinks::http::{HttpMethod, HttpSinkConfig},
    sinks::util::{BatchBytesConfig, Compression, TowerRequestConfig},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct LogdnaConfig {
    api_key: String,
    host: Option<String>,

    // Tags
    hostname: String,
    mac: Option<String>,
    ip: Option<String>,

    // TODO: batch type I think needs to be event?
    //
    #[serde(default)]
    request: TowerRequestConfig,
}

inventory::submit! {
    SinkDescription::new::<LogdnaConfig>("logdna")
}

#[typetag::serde(name = "logdna")]
impl SinkConfig for LogdnaConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        todo!()
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "logdna"
    }
}

fn encode_event(event: Event) -> Option<Vec<u8>> {
    let log = event.as_log();

    let line = log
        .get(&event::MESSAGE)
        .map(Clone::clone)
        .unwrap_or_else(|| "".into());
    let app = log.get(&"app".into()).unwrap_or_else(|| todo!(""));
    // let level = event

    let json = serde_json::json!({
       "line": line,
       "app": app
    });

    serde_json::to_vec(&json)
        .map(|mut v| {
            v.push(b',');
            v
        })
        .map_err(|e| panic!("Unable to encode into JSON: {}", e))
        .ok()
}
