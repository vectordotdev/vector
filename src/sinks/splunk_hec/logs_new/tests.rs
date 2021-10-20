use crate::event::Event;
use crate::sinks::splunk_hec::logs_new::config::HecSinkLogsConfig;
use crate::sinks::util::http::HttpSink;
// use crate::sinks::util::test::load_sink;
use chrono::Utc;
use serde::Deserialize;
use std::collections::BTreeMap;
use vector_core::config::log_schema;

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

// #[test]
// fn splunk_encode_log_event_json() {
//     let mut event = Event::from("hello world");
//     event.as_mut_log().insert("key", "value");
//     event.as_mut_log().insert("magic", "vector");

//     let (config, _cx) = load_sink::<HecSinkLogsConfig>(
//         r#"
//         host = "test.com"
//         token = "alksjdfo"
//         host_key = "host"
//         indexed_fields = ["key"]
//         source = "{{ magic }}"

//         [encoding]
//         codec = "json"
//         except_fields = ["magic"]
//     "#,
//     )
//     .unwrap();

//     let bytes = config.encode_event(event).unwrap();

//     let hec_event = serde_json::from_slice::<HecEventJson>(&bytes[..]).unwrap();

//     let event = &hec_event.event;
//     let kv = event.get(&"key".to_string()).unwrap();

//     assert_eq!(kv, &"value".to_string());
//     assert_eq!(
//         event[&log_schema().message_key().to_string()],
//         "hello world".to_string()
//     );
//     assert!(event
//         .get(&log_schema().timestamp_key().to_string())
//         .is_none());

//     assert!(!event.contains_key("magic"));
//     assert_eq!(hec_event.source, Some("vector".to_string()));

//     assert_eq!(
//         hec_event.fields.get("key").map(|s| s.as_str()),
//         Some("value")
//     );

//     let now = Utc::now().timestamp_millis() as f64 / 1000f64;
//     assert!(
//         (hec_event.time - now).abs() < 0.2,
//         "hec_event.time = {}, now = {}",
//         hec_event.time,
//         now
//     );
//     assert_eq!((hec_event.time * 1000f64).fract(), 0f64);
// }

// #[test]
// fn splunk_encode_log_event_text() {
//     let mut event = Event::from("hello world");
//     event.as_mut_log().insert("key", "value");

//     let (config, _cx) = load_sink::<HecSinkLogsConfig>(
//         r#"
//         host = "test.com"
//         token = "alksjdfo"
//         host_key = "host"
//         indexed_fields = ["key"]

//         [encoding]
//         codec = "text"
//     "#,
//     )
//     .unwrap();

//     let bytes = config.encode_event(event).unwrap();

//     let hec_event = serde_json::from_slice::<HecEventText>(&bytes[..]).unwrap();

//     assert_eq!(hec_event.event.as_str(), "hello world");

//     assert_eq!(
//         hec_event.fields.get("key").map(|s| s.as_str()),
//         Some("value")
//     );

//     let now = Utc::now().timestamp_millis() as f64 / 1000f64;
//     assert!(
//         (hec_event.time - now).abs() < 0.2,
//         "hec_event.time = {}, now = {}",
//         hec_event.time,
//         now
//     );
//     assert_eq!((hec_event.time * 1000f64).fract(), 0f64);
// }
