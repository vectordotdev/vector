use crate::event::Event;
use crate::sinks::splunk_hec::logs::encoder::HecLogsEncoder;
use crate::sinks::splunk_hec::logs::sink::process_log;
use crate::template::Template;
use chrono::Utc;
use serde::Deserialize;
use vector_core::ByteSizeOf;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use vector_core::config::log_schema;

use super::sink::ProcessedEvent;

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

fn get_processed_event() -> ProcessedEvent {
    let mut event = Event::from("hello world");
    event
        .as_mut_log()
        .insert("event_sourcetype", "test_sourcetype");
    event.as_mut_log().insert("event_source", "test_source");
    event.as_mut_log().insert("event_index", "test_index");
    event.as_mut_log().insert("event_field1", "test_value1");
    event.as_mut_log().insert("event_field2", "test_value2");
    event.as_mut_log().insert("key", "value");
    let event_byte_size = event.size_of();

    let sourcetype = Template::try_from("{{ event_sourcetype }}".to_string()).ok();
    let source = Template::try_from("{{ event_source }}".to_string()).ok();
    let index = Template::try_from("{{ event_index }}".to_string()).ok();
    let indexed_fields = vec!["event_field1".to_string(), "event_field2".to_string()];

    process_log(
        event.into_log(),
        event_byte_size,
        sourcetype.as_ref(),
        source.as_ref(),
        index.as_ref(),
        "host_key",
        indexed_fields.as_slice(),
    )
    .unwrap()
}

#[test]
fn splunk_process_log_event() {
    let processed_event = get_processed_event();

    assert_eq!(
        processed_event.sourcetype,
        Some("test_sourcetype".to_string())
    );
    assert_eq!(processed_event.source, Some("test_source".to_string()));
    assert_eq!(processed_event.index, Some("test_index".to_string()));
    assert!(processed_event.fields.contains("event_field1"));
    assert!(processed_event.fields.contains("event_field2"));
}

#[test]
fn splunk_encode_log_event_json() {
    let processed_event = get_processed_event();
    let encoder = HecLogsEncoder::Json;
    let bytes = encoder.encode_event(processed_event).unwrap();
    let hec_event = serde_json::from_slice::<HecEventJson>(&bytes[..]).unwrap();
    let event = &hec_event.event;

    assert_eq!(event.get(&"key".to_string()).unwrap(), &"value".to_string());
    assert_eq!(
        event[&log_schema().message_key().to_string()],
        "hello world".to_string()
    );
    assert!(event
        .get(&log_schema().timestamp_key().to_string())
        .is_none());

    assert_eq!(hec_event.source, Some("test_source".to_string()));

    assert_eq!(
        hec_event.fields.get("event_field1").map(|s| s.as_str()),
        Some("test_value1")
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
    let processed_event = get_processed_event();
    let encoder = HecLogsEncoder::Text;
    let bytes = encoder.encode_event(processed_event).unwrap();

    let hec_event = serde_json::from_slice::<HecEventText>(&bytes[..]).unwrap();

    assert_eq!(hec_event.event.as_str(), "hello world");

    assert_eq!(
        hec_event.fields.get("event_field1").map(|s| s.as_str()),
        Some("test_value1")
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
