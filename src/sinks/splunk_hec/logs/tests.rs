use crate::event::Event;
use crate::sinks::splunk_hec::logs::encoder::HecLogsEncoder;
use crate::sinks::splunk_hec::logs::sink::process_log;
use crate::sinks::util::processed_event::ProcessedEvent;
use crate::template::Template;
use chrono::Utc;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use vector_core::config::log_schema;
use vector_core::event::{LogEvent, Value};
use vector_core::ByteSizeOf;

use super::sink::HecLogsProcessedEventMetadata;

#[derive(Deserialize, Debug)]
struct HecEventJson {
    time: f64,
    event: BTreeMap<String, String>,
    fields: BTreeMap<String, String>,
    source: Option<String>,
    sourcetype: Option<String>,
    index: Option<String>,
    host: Option<String>,
}

#[derive(Deserialize, Debug)]
struct HecEventText {
    time: f64,
    event: String,
    fields: BTreeMap<String, String>,
    source: Option<String>,
    sourcetype: Option<String>,
    index: Option<String>,
    host: Option<String>,
}

fn get_processed_event() -> ProcessedEvent<LogEvent, HecLogsProcessedEventMetadata> {
    let mut event = Event::from("hello world");
    event
        .as_mut_log()
        .insert("event_sourcetype", "test_sourcetype");
    event.as_mut_log().insert("event_source", "test_source");
    event.as_mut_log().insert("event_index", "test_index");
    event.as_mut_log().insert("host_key", "test_host");
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
    let metadata = processed_event.metadata;

    assert_eq!(metadata.sourcetype, Some("test_sourcetype".to_string()));
    assert_eq!(metadata.source, Some("test_source".to_string()));
    assert_eq!(metadata.index, Some("test_index".to_string()));
    assert_eq!(metadata.host, Some(Value::from("test_host")));
    assert!(metadata.fields.contains("event_field1"));
    assert!(metadata.fields.contains("event_field2"));
}

#[test]
fn splunk_encode_log_event_json() {
    let processed_event = get_processed_event();
    let encoder = HecLogsEncoder::Json;
    let bytes = encoder.encode_event(processed_event).unwrap();
    let hec_data = serde_json::from_slice::<HecEventJson>(&bytes[..]).unwrap();
    let event = hec_data.event;

    assert_eq!(event.get("key").unwrap(), "value");
    assert_eq!(
        event.get(&log_schema().message_key().to_string()).unwrap(),
        "hello world"
    );
    assert!(event
        .get(&log_schema().timestamp_key().to_string())
        .is_none());

    assert_eq!(hec_data.source, Some("test_source".to_string()));
    assert_eq!(hec_data.sourcetype, Some("test_sourcetype".to_string()));
    assert_eq!(hec_data.index, Some("test_index".to_string()));
    assert_eq!(hec_data.host, Some("test_host".to_string()));

    assert_eq!(hec_data.fields.get("event_field1").unwrap(), "test_value1");

    let now = Utc::now().timestamp_millis() as f64 / 1000f64;
    assert!(
        (hec_data.time - now).abs() < 0.2,
        "hec_data.time = {}, now = {}",
        hec_data.time,
        now
    );
    assert_eq!((hec_data.time * 1000f64).fract(), 0f64);
}

#[test]
fn splunk_encode_log_event_text() {
    let processed_event = get_processed_event();
    let encoder = HecLogsEncoder::Text;
    let bytes = encoder.encode_event(processed_event).unwrap();
    let hec_data = serde_json::from_slice::<HecEventText>(&bytes[..]).unwrap();

    assert_eq!(hec_data.event.as_str(), "hello world");

    assert_eq!(hec_data.source, Some("test_source".to_string()));
    assert_eq!(hec_data.sourcetype, Some("test_sourcetype".to_string()));
    assert_eq!(hec_data.index, Some("test_index".to_string()));
    assert_eq!(hec_data.host, Some("test_host".to_string()));

    assert_eq!(hec_data.fields.get("event_field1").unwrap(), "test_value1");

    let now = Utc::now().timestamp_millis() as f64 / 1000f64;
    assert!(
        (hec_data.time - now).abs() < 0.2,
        "hec_data.time = {}, now = {}",
        hec_data.time,
        now
    );
    assert_eq!((hec_data.time * 1000f64).fract(), 0f64);
}
