use std::{collections::BTreeMap, sync::Arc};

use chrono::{TimeZone, Utc};
use futures_util::StreamExt;
use serde::Deserialize;
use vector_core::{
    config::log_schema,
    event::{Event, Value},
    ByteSizeOf,
};

use super::sink::HecProcessedEvent;
use crate::{
    config::{SinkConfig, SinkContext},
    sinks::{
        splunk_hec::logs::{config::HecLogsSinkConfig, encoder::HecLogsEncoder, sink::process_log},
        util::{test::build_test_server, Compression},
    },
    template::Template,
    test_util::next_addr,
};

#[derive(Deserialize, Debug)]
struct HecEventJson {
    time: f64,
    event: BTreeMap<String, serde_json::Value>,
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

fn get_processed_event() -> HecProcessedEvent {
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
    event.as_mut_log().insert("int_val", 123);
    event.as_mut_log().insert(
        log_schema().timestamp_key(),
        Utc.timestamp_nanos(1638366107111456123),
    );
    let event_byte_size = event.size_of();

    let sourcetype = Template::try_from("{{ event_sourcetype }}".to_string()).ok();
    let source = Template::try_from("{{ event_source }}".to_string()).ok();
    let index = Template::try_from("{{ event_index }}".to_string()).ok();
    let indexed_fields = vec!["event_field1".to_string(), "event_field2".to_string()];
    let timestamp_nanos_key = Some(String::from("ts_nanos_key"));

    process_log(
        event.into_log(),
        event_byte_size,
        sourcetype.as_ref(),
        source.as_ref(),
        index.as_ref(),
        "host_key",
        indexed_fields.as_slice(),
        timestamp_nanos_key.as_deref(),
    )
    .unwrap()
}

fn get_event_with_token(msg: &str, token: &str) -> Event {
    let mut event = Event::from(msg);
    event
        .metadata_mut()
        .set_splunk_hec_token(Some(Arc::from(token)));
    event
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

    assert_eq!(event.get("key").unwrap(), &serde_json::Value::from("value"));
    assert_eq!(event.get("int_val").unwrap(), &serde_json::Value::from(123));
    assert_eq!(
        event.get(&log_schema().message_key().to_string()).unwrap(),
        &serde_json::Value::from("hello world")
    );
    assert!(event
        .get(&log_schema().timestamp_key().to_string())
        .is_none());

    assert_eq!(hec_data.source, Some("test_source".to_string()));
    assert_eq!(hec_data.sourcetype, Some("test_sourcetype".to_string()));
    assert_eq!(hec_data.index, Some("test_index".to_string()));
    assert_eq!(hec_data.host, Some("test_host".to_string()));

    assert_eq!(hec_data.fields.get("event_field1").unwrap(), "test_value1");

    assert_eq!(hec_data.time, 1638366107.111);
    assert_eq!(
        event.get("ts_nanos_key").unwrap(),
        &serde_json::Value::from(456123)
    );
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

    assert_eq!(hec_data.time, 1638366107.111);
}

#[tokio::test]
async fn splunk_passthrough_token() {
    let addr = next_addr();
    let config = HecLogsSinkConfig {
        default_token: "token".into(),
        endpoint: format!("http://{}", addr),
        host_key: "host".into(),
        indexed_fields: Vec::new(),
        index: None,
        sourcetype: None,
        source: None,
        encoding: HecLogsEncoder::Json.into(),
        compression: Compression::None,
        batch: Default::default(),
        request: Default::default(),
        tls: None,
        acknowledgements: Default::default(),
        timestamp_nanos_key: None,
    };
    let cx = SinkContext::new_test();

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = build_test_server(addr);
    tokio::spawn(server);

    let events = vec![
        get_event_with_token("message-1", "passthrough-token-1"),
        get_event_with_token("message-2", "passthrough-token-2"),
        Event::from("default token will be used"),
    ];

    let _ = sink.run_events(events).await.unwrap();

    let mut tokens = rx
        .take(3)
        .map(|r| r.0.headers.get("Authorization").unwrap().clone())
        .collect::<Vec<_>>()
        .await;

    tokens.sort();
    assert_eq!(
        tokens,
        vec![
            "Splunk passthrough-token-1",
            "Splunk passthrough-token-2",
            "Splunk token"
        ]
    )
}
