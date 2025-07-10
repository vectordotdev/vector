use std::{collections::BTreeMap, sync::Arc};

use chrono::{TimeZone, Utc};
use futures_util::StreamExt;
use serde::{de, Deserialize};
use vector_lib::codecs::{JsonSerializerConfig, TextSerializerConfig};
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::event::EventMetadata;
use vector_lib::lookup::lookup_v2::OptionalTargetPath;
use vector_lib::schema::{meaning, Definition};
use vector_lib::{
    config::log_schema,
    event::{Event, LogEvent, Value},
};
use vrl::path::OwnedTargetPath;
use vrl::value::Kind;
use vrl::{event_path, metadata_path, owned_value_path};

use super::sink::{HecLogsProcessedEventMetadata, HecProcessedEvent};
use crate::sinks::util::processed_event::ProcessedEvent;
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

fn get_encoded_event<D: de::DeserializeOwned>(
    encoding: EncodingConfig,
    processed_event: ProcessedEvent<LogEvent, HecLogsProcessedEventMetadata>,
) -> D {
    let encoder = hec_encoder(encoding);
    let mut bytes = Vec::new();
    encoder
        .encode_input(vec![processed_event], &mut bytes)
        .unwrap();
    serde_json::from_slice::<D>(&bytes).unwrap()
}

fn get_processed_event_timestamp(
    timestamp: Option<Value>,
    timestamp_key: Option<OptionalTargetPath>,
    auto_extract_timestamp: bool,
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

    if let Some(OptionalTargetPath {
        path: Some(ts_path),
    }) = &timestamp_key
    {
        if timestamp.is_some() {
            event
                .as_mut_log()
                .insert(&OwnedTargetPath::event(ts_path.path.clone()), timestamp);
        } else {
            event
                .as_mut_log()
                .remove(&OwnedTargetPath::event(ts_path.path.clone()));
        }
    }

    let sourcetype = Template::try_from("{{ event_sourcetype }}".to_string()).ok();
    let source = Template::try_from("{{ event_source }}".to_string()).ok();
    let index = Template::try_from("{{ event_index }}".to_string()).ok();
    let indexed_fields = vec![
        owned_value_path!("event_field1"),
        owned_value_path!("event_field2"),
    ];
    let timestamp_nanos_key = Some(String::from("ts_nanos_key"));

    process_log(
        event,
        &super::sink::HecLogData {
            sourcetype: sourcetype.as_ref(),
            source: source.as_ref(),
            index: index.as_ref(),
            host_key: Some(OptionalTargetPath {
                path: Some(OwnedTargetPath::event(owned_value_path!("host_key"))),
            }),
            indexed_fields: indexed_fields.as_slice(),
            timestamp_nanos_key: timestamp_nanos_key.as_ref(),
            timestamp_key,
            endpoint_target: EndpointTarget::Event,
            auto_extract_timestamp,
        },
    )
}

fn get_processed_event() -> HecProcessedEvent {
    get_processed_event_timestamp(
        Some(vrl::value::Value::Timestamp(
            Utc.timestamp_nanos(1638366107111456123),
        )),
        Some(OptionalTargetPath {
            path: Some(OwnedTargetPath::event(owned_value_path!("timestamp"))),
        }),
        false,
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
    let hec_data =
        get_encoded_event::<HecEventJson>(JsonSerializerConfig::default().into(), processed_event);
    let event = hec_data.event;

    assert_eq!(event.get("key").unwrap(), &serde_json::Value::from("value"));
    assert_eq!(event.get("int_val").unwrap(), &serde_json::Value::from(123));
    assert_eq!(
        event
            .get(&log_schema().message_key().unwrap().to_string())
            .unwrap(),
        &serde_json::Value::from("hello world")
    );
    assert!(!event.contains_key(log_schema().timestamp_key().unwrap().to_string().as_str()));

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
    let hec_data =
        get_encoded_event::<HecEventText>(TextSerializerConfig::default().into(), processed_event);

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
        host_key: None,
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
        timestamp_key: None,
        auto_extract_timestamp: None,
        endpoint_target: EndpointTarget::Event,
    };
    let cx = SinkContext::default();

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
        timestamp_path: Option<OptionalTargetPath>,
        auto_extract_timestamp: bool,
    ) -> HecEventJson {
        let processed_event =
            get_processed_event_timestamp(timestamp, timestamp_path, auto_extract_timestamp);
        get_encoded_event::<HecEventJson>(JsonSerializerConfig::default().into(), processed_event)
    }

    let timestamp_key = Some(OptionalTargetPath {
        path: Some(OwnedTargetPath::event(owned_value_path!("timestamp"))),
    });

    let no_timestamp = Some(OptionalTargetPath::none());
    let dont_auto_extract = false;
    let do_auto_extract = true;

    // no timestamp_key is provided
    let mut hec_data = get_hec_data_for_timestamp_test(None, no_timestamp, dont_auto_extract);
    assert_eq!(hec_data.time, None);

    // timestamp_key is provided but timestamp is not valid type
    hec_data = get_hec_data_for_timestamp_test(
        Some(vrl::value::Value::Integer(0)),
        timestamp_key.clone(),
        dont_auto_extract,
    );
    assert_eq!(hec_data.time, None);

    // timestamp_key is provided but no timestamp in the event
    hec_data = get_hec_data_for_timestamp_test(None, timestamp_key.clone(), dont_auto_extract);
    assert_eq!(hec_data.time, None);

    // timestamp_key is provided and timestamp is valid
    hec_data = get_hec_data_for_timestamp_test(
        Some(Value::Timestamp(Utc::now())),
        timestamp_key.clone(),
        dont_auto_extract,
    );
    assert!(hec_data.time.is_some());

    // timestamp_key is provided and timestamp is valid, but auto_extract_timestamp is set
    hec_data = get_hec_data_for_timestamp_test(
        Some(Value::Timestamp(Utc::now())),
        timestamp_key.clone(),
        do_auto_extract,
    );
    assert_eq!(hec_data.time, None);
}

#[test]
fn splunk_encode_log_event_semantic_meanings() {
    let metadata = EventMetadata::default().with_schema_definition(&Arc::new(
        Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
            .with_source_metadata(
                "splunk_hec",
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("hostname"))),
                &owned_value_path!("hostname"),
                Kind::bytes(),
                Some(meaning::HOST),
            )
            .with_source_metadata(
                "splunk_hec",
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("timestamp"))),
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some(meaning::TIMESTAMP),
            ),
    ));

    let mut log = LogEvent::new_with_metadata(metadata);
    log.insert(event_path!("message"), "the_message");

    // insert an arbitrary metadata field such that the log becomes Vector namespaced
    log.insert(metadata_path!("vector", "foo"), "bar");

    let og_time = Utc::now();

    // determine the time we expect to get after encoding
    let expected_time = (og_time.timestamp_millis() as f64) / 1000f64;

    log.insert(metadata_path!("splunk_hec", "hostname"), "roast");
    log.insert(
        metadata_path!("splunk_hec", "timestamp"),
        Value::Timestamp(og_time),
    );

    assert!(log.namespace() == LogNamespace::Vector);

    let event = Event::Log(log);

    let processed_event = process_log(
        event,
        &super::sink::HecLogData {
            sourcetype: None,
            source: None,
            index: None,
            host_key: None,
            indexed_fields: &[],
            timestamp_nanos_key: None,
            timestamp_key: None,
            endpoint_target: EndpointTarget::Event,
            auto_extract_timestamp: false,
        },
    );

    let hec_data =
        get_encoded_event::<HecEventJson>(JsonSerializerConfig::default().into(), processed_event);

    assert_eq!(hec_data.time, Some(expected_time));

    assert_eq!(hec_data.host, Some("roast".to_string()));
}
