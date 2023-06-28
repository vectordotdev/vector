use std::{collections::BTreeMap, sync::Arc};

use chrono::{TimeZone, Utc};
use codecs::{JsonSerializerConfig, TextSerializerConfig};
use futures_util::StreamExt;
use lookup::lookup_v2::OptionalValuePath;
use lookup::{OwnedValuePath, PathPrefix};
use serde::Deserialize;
use vector_core::{
    config::log_schema,
    event::{Event, LogEvent, Value},
};

use super::sink::HecProcessedEvent;
use crate::sinks::splunk_hec::common::config_timestamp_key;
use crate::{
    codecs::{Encoder, EncodingConfig},
    config::{SinkConfig, SinkContext},
    sinks::{
        splunk_hec::{
            common::EndpointTarget,
            logs::{config::HecLogsSinkConfig, encoder::HecLogsEncoder, sink::process_log},
        },
        util::{encoding::Encoder as _, test::build_test_server, Compression},
    },
    template::Template,
    test_util::next_addr,
};

#[derive(Deserialize, Debug)]
struct HecEventJson {
    time: Option<f64>,
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

fn get_processed_event_timestamp(
    timestamp: Option<Value>,
    timestamp_key: Option<OwnedValuePath>,
) -> HecProcessedEvent {
    let mut event = Event::Log(LogEvent::from("hello world"));
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

    if let Some(timestamp_key) = &timestamp_key {
        if timestamp.is_some() {
            event
                .as_mut_log()
                .insert((PathPrefix::Event, timestamp_key), timestamp);
        } else {
            event
                .as_mut_log()
                .remove((PathPrefix::Event, timestamp_key));
        }
    }

    let sourcetype = Template::try_from("{{ event_sourcetype }}".to_string()).ok();
    let source = Template::try_from("{{ event_source }}".to_string()).ok();
    let index = Template::try_from("{{ event_index }}".to_string()).ok();
    let indexed_fields = vec!["event_field1".to_string(), "event_field2".to_string()];
    let timestamp_nanos_key = Some(String::from("ts_nanos_key"));

    process_log(
        event,
        &super::sink::HecLogData {
            sourcetype: sourcetype.as_ref(),
            source: source.as_ref(),
            index: index.as_ref(),
            host_key: "host_key",
            indexed_fields: indexed_fields.as_slice(),
            timestamp_nanos_key: timestamp_nanos_key.as_ref(),
            timestamp_key,
            endpoint_target: EndpointTarget::Event,
        },
    )
}

fn get_processed_event() -> HecProcessedEvent {
    get_processed_event_timestamp(
        Some(vrl::value::Value::Timestamp(
            Utc.timestamp_nanos(1638366107111456123),
        )),
        config_timestamp_key().path,
    )
}

fn get_event_with_token(msg: &str, token: &str) -> Event {
    let mut event = Event::Log(LogEvent::from(msg));
    event.metadata_mut().set_splunk_hec_token(Arc::from(token));
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

fn hec_encoder(encoding: EncodingConfig) -> HecLogsEncoder {
    let transformer = encoding.transformer();
    let serializer = encoding.build().unwrap();
    let encoder = Encoder::<()>::new(serializer);
    HecLogsEncoder {
        transformer,
        encoder,
        auto_extract_timestamp: false,
    }
}

#[test]
fn splunk_encode_log_event_json() {
    let processed_event = get_processed_event();
    let encoder = hec_encoder(JsonSerializerConfig::default().into());
    let mut bytes = Vec::new();
    encoder
        .encode_input(vec![processed_event], &mut bytes)
        .unwrap();
    let hec_data = serde_json::from_slice::<HecEventJson>(&bytes).unwrap();
    let event = hec_data.event;

    assert_eq!(event.get("key").unwrap(), &serde_json::Value::from("value"));
    assert_eq!(event.get("int_val").unwrap(), &serde_json::Value::from(123));
    assert_eq!(
        event.get(&log_schema().message_key().to_string()).unwrap(),
        &serde_json::Value::from("hello world")
    );
    assert!(event
        .get(log_schema().timestamp_key().unwrap().to_string().as_str())
        .is_none());

    assert_eq!(hec_data.source, Some("test_source".to_string()));
    assert_eq!(hec_data.sourcetype, Some("test_sourcetype".to_string()));
    assert_eq!(hec_data.index, Some("test_index".to_string()));
    assert_eq!(hec_data.host, Some("test_host".to_string()));

    assert_eq!(hec_data.fields.get("event_field1").unwrap(), "test_value1");

    assert_eq!(hec_data.time, Some(1638366107.111));
    assert_eq!(
        event.get("ts_nanos_key").unwrap(),
        &serde_json::Value::from(456123)
    );
}

#[test]
fn splunk_encode_log_event_text() {
    let processed_event = get_processed_event();
    let encoder = hec_encoder(TextSerializerConfig::default().into());
    let mut bytes = Vec::new();
    encoder
        .encode_input(vec![processed_event], &mut bytes)
        .unwrap();
    let hec_data = serde_json::from_slice::<HecEventText>(&bytes).unwrap();

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
        default_token: "token".to_string().into(),
        endpoint: format!("http://{}", addr),
        host_key: "host".into(),
        indexed_fields: Vec::new(),
        index: None,
        sourcetype: None,
        source: None,
        encoding: JsonSerializerConfig::default().into(),
        compression: Compression::None,
        batch: Default::default(),
        request: Default::default(),
        tls: None,
        acknowledgements: Default::default(),
        timestamp_nanos_key: None,
        timestamp_key: OptionalValuePath {
            path: log_schema().timestamp_key().cloned(),
        },
        auto_extract_timestamp: None,
        endpoint_target: EndpointTarget::Event,
    };
    let cx = SinkContext::new_test();

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = build_test_server(addr);
    tokio::spawn(server);

    let events = vec![
        get_event_with_token("message-1", "passthrough-token-1"),
        get_event_with_token("message-2", "passthrough-token-2"),
        Event::Log(LogEvent::from("default token will be used")),
    ];

    sink.run_events(events).await.unwrap();

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

#[test]
fn splunk_encode_log_event_json_timestamps() {
    crate::test_util::trace_init();

    fn get_hec_data_for_timestamp_test(
        timestamp: Option<Value>,
        timestamp_key: Option<OwnedValuePath>,
    ) -> HecEventJson {
        let processed_event = get_processed_event_timestamp(timestamp, timestamp_key);
        let encoder = hec_encoder(JsonSerializerConfig::default().into());
        let mut bytes = Vec::new();
        encoder
            .encode_input(vec![processed_event], &mut bytes)
            .unwrap();
        serde_json::from_slice::<HecEventJson>(&bytes).unwrap()
    }

    let timestamp = lookup::owned_value_path!("timestamp");

    // no timestamp_key is provided
    let mut hec_data = get_hec_data_for_timestamp_test(None, None);
    assert_eq!(hec_data.time, None);

    // timestamp_key is provided but timestamp is not valid type
    hec_data = get_hec_data_for_timestamp_test(
        Some(vrl::value::Value::Integer(0)),
        Some(timestamp.clone()),
    );
    assert_eq!(hec_data.time, None);

    // timestamp_key is provided but no timestamp in the event
    let hec_data = get_hec_data_for_timestamp_test(None, Some(timestamp));
    assert_eq!(hec_data.time, None);
}
