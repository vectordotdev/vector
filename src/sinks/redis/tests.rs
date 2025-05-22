use std::collections::HashMap;

use vector_lib::codecs::{JsonSerializerConfig, TextSerializerConfig};
use vector_lib::event::{LogEvent, Metric, MetricKind, MetricValue};
use vector_lib::request_metadata::GroupedCountByteSize;

use super::{config::RedisSinkConfig, request_builder::encode_event};
use crate::{
    codecs::{Encoder, Transformer},
    config::log_schema,
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<RedisSinkConfig>();
}

#[test]
fn redis_log_event_json() {
    let msg = "hello_world".to_owned();
    let mut byte_size = GroupedCountByteSize::new_untagged();
    let mut evt = LogEvent::from(msg.clone());
    evt.insert("key", "value");
    let result = encode_event(
        evt.into(),
        "key".to_string(),
        &Default::default(),
        &mut Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
        &mut byte_size,
    )
    .unwrap()
    .value;
    let map: HashMap<String, String> = serde_json::from_slice(&result[..]).unwrap();
    assert_eq!(msg, map[&log_schema().message_key().unwrap().to_string()]);
}

#[test]
fn redis_log_event_text() {
    let msg = "hello_world".to_owned();
    let evt = LogEvent::from(msg.clone());
    let mut byte_size = GroupedCountByteSize::new_untagged();
    let event = encode_event(
        evt.into(),
        "key".to_string(),
        &Default::default(),
        &mut Encoder::<()>::new(TextSerializerConfig::default().build().into()),
        &mut byte_size,
    )
    .unwrap()
    .value;
    assert_eq!(event, Vec::from(msg));
}

#[test]
fn redis_log_encode_event() {
    let msg = "hello_world";
    let mut evt = LogEvent::from(msg);
    let mut byte_size = GroupedCountByteSize::new_untagged();
    evt.insert("key", "value");

    let result = encode_event(
        evt.into(),
        "key".to_string(),
        &Transformer::new(None, Some(vec!["key".into()]), None).unwrap(),
        &mut Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
        &mut byte_size,
    )
    .unwrap()
    .value;

    let map: HashMap<String, String> = serde_json::from_slice(&result[..]).unwrap();
    assert!(!map.contains_key("key"));
}

#[test]
fn redis_metric_encode_event() {
    let mut byte_size = GroupedCountByteSize::new_untagged();
    let metric = Metric::new(
        "test_counter",
        MetricKind::Absolute,
        MetricValue::Counter { value: 42.0 },
    );

    let result = encode_event(
        metric.into(),
        "metrics.counter".to_string(),
        &Default::default(),
        &mut Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
        &mut byte_size,
    )
    .unwrap()
    .value;

    let json: serde_json::Value = serde_json::from_slice(&result).unwrap();

    assert_eq!(json["name"], "test_counter");
    assert_eq!(json["kind"], "absolute");
    assert_eq!(json["counter"]["value"], 42.0);
}
