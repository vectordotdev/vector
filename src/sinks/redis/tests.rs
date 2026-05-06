use std::collections::HashMap;

use vector_lib::{
    codecs::{JsonSerializerConfig, TextSerializerConfig},
    event::{LogEvent, Metric, MetricKind, MetricValue},
    request_metadata::GroupedCountByteSize,
};

use super::{
    DataType,
    config::{ListMethod, RedisSinkConfig, SortedSetMethod},
    request_builder::encode_event,
    service::RedisResponse,
};
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
        None,
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
        None,
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
        None,
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
        None,
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

#[test]
fn redis_log_scoring() {
    let msg = "hello_world";
    let mut evt = LogEvent::from(msg);
    let mut byte_size = GroupedCountByteSize::new_untagged();
    evt.insert("key", "value");

    let result = encode_event(
        evt.into(),
        "key".to_string(),
        Some(64),
        &Default::default(),
        &mut Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
        &mut byte_size,
    )
    .unwrap()
    .score;

    assert_eq!(result, Some(64));
}

// Redis PUBLISH returns the number of subscribers that received the message as an integer.
// redis-rs deserializes integer 0 as bool false, which would cause is_successful() to return
// false and trigger an infinite retry loop when no subscribers are connected.
#[test]
fn redis_channel_publish_zero_subscribers_is_successful() {
    let response = RedisResponse {
        event_status: vec![false], // 0 subscribers → redis-rs deserializes as false
        data_type: DataType::Channel,
        events_byte_size: GroupedCountByteSize::new_untagged(),
        byte_size: 0,
    };
    assert!(
        response.is_successful(),
        "Channel publish with 0 subscribers should be treated as success"
    );
}

#[test]
fn redis_list_and_sorted_set_still_check_event_status() {
    for data_type in [
        DataType::List(ListMethod::RPush),
        DataType::SortedSet(SortedSetMethod::ZAdd),
    ] {
        let response = RedisResponse {
            event_status: vec![false],
            data_type,
            events_byte_size: GroupedCountByteSize::new_untagged(),
            byte_size: 0,
        };
        assert!(
            !response.is_successful(),
            "{data_type:?} with false event_status should not be successful"
        );
    }
}
