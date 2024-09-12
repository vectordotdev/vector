use std::{collections::HashMap, convert::TryFrom, num::NonZeroU32, time::SystemTime};

use chrono::{DateTime, Utc};
use futures::{future::ready, stream};
use serde::Deserialize;
use vector_lib::config::{init_telemetry, Tags, Telemetry};

use super::*;
use crate::{
    config::{GenerateConfig, SinkConfig, SinkContext},
    event::{Event, KeyString, LogEvent, Metric, MetricKind, MetricValue, Value},
    test_util::{
        components::{
            run_and_assert_data_volume_sink_compliance, run_and_assert_sink_compliance,
            DATA_VOLUME_SINK_TAGS, SINK_TAGS,
        },
        http::{always_200_response, spawn_blackhole_http_server},
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<NewRelicConfig>();
}

async fn sink() -> (VectorSink, Event) {
    let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

    let config = NewRelicConfig::generate_config().to_string();
    let mut config = NewRelicConfig::deserialize(toml::de::ValueDeserializer::new(&config))
        .expect("config should be valid");
    config.override_uri = Some(mock_endpoint);

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();

    let event = Event::Log(LogEvent::from("simple message"));

    (sink, event)
}

#[tokio::test]
async fn component_spec_compliance() {
    let (sink, event) = sink().await;
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
}

#[tokio::test]
async fn component_spec_compliance_data_volume() {
    // We need to configure Vector to emit the service and source tags.
    // The default is to not emit these.
    init_telemetry(
        Telemetry {
            tags: Tags {
                emit_service: true,
                emit_source: true,
            },
        },
        true,
    );

    let (sink, event) = sink().await;
    run_and_assert_data_volume_sink_compliance(
        sink,
        stream::once(ready(event)),
        &DATA_VOLUME_SINK_TAGS,
    )
    .await;
}

macro_rules! model_map {
    ( $( $key:literal :  $value:expr , )* ) => {
        [ $( ( KeyString::from($key), Value::from($value) ), )* ]
            .into_iter()
            .collect::<HashMap<_, _>>()
    }
}

#[test]
fn generates_event_api_model_without_message_field() {
    let mut map = HashMap::<KeyString, Value>::new();
    map.insert("eventType".into(), Value::from("TestEvent".to_owned()));
    map.insert("user".into(), Value::from("Joe".to_owned()));
    map.insert("user_id".into(), Value::from(123456));
    let event = Event::Log(LogEvent::from(map));
    let model =
        EventsApiModel::try_from(vec![event]).expect("Failed mapping events into API model");

    assert_eq!(
        &model.0[..],
        &[model_map! {
            "eventType": "TestEvent",
            "user": "Joe",
            "user_id": 123456,
        }]
    );
}

#[test]
fn generates_event_api_model_with_message_field() {
    let mut map = HashMap::<KeyString, Value>::new();
    map.insert("eventType".into(), Value::from("TestEvent".to_owned()));
    map.insert("user".into(), Value::from("Joe".to_owned()));
    map.insert("user_id".into(), Value::from(123456));
    map.insert(
        "message".into(),
        Value::from("This is a message".to_owned()),
    );
    let event = Event::Log(LogEvent::from(map));
    let model =
        EventsApiModel::try_from(vec![event]).expect("Failed mapping events into API model");

    assert_eq!(
        &model.0[..],
        &[model_map! {
            "eventType": "TestEvent",
            "user": "Joe",
            "user_id": 123456,
            "message": "This is a message",
        }]
    );
}

#[test]
fn generates_event_api_model_with_json_inside_message_field() {
    let mut map = HashMap::<KeyString, Value>::new();
    map.insert("eventType".into(), Value::from("TestEvent".to_owned()));
    map.insert("user".into(), Value::from("Joe".to_owned()));
    map.insert("user_id".into(), Value::from(123456));
    map.insert(
        "message".into(),
        Value::from("{\"my_key\" : \"my_value\"}".to_owned()),
    );
    let event = Event::Log(LogEvent::from(map));
    let model =
        EventsApiModel::try_from(vec![event]).expect("Failed mapping events into API model");

    assert_eq!(
        &model.0[..],
        &[model_map! {
            "eventType": "TestEvent",
            "user": "Joe",
            "user_id": 123456,
            "my_key": "my_value",
        }]
    );
}

#[test]
fn generates_log_api_model_without_message_field() {
    let mut map = HashMap::<KeyString, Value>::new();
    map.insert("tag_key".into(), Value::from("tag_value".to_owned()));
    let event = Event::Log(LogEvent::from(map));
    let model = LogsApiModel::try_from(vec![event]).expect("Failed mapping logs into API model");
    let logs = model.0[0].get("logs").expect("Logs data store not present");

    assert_eq!(
        &logs[..],
        &[model_map! {
            "tag_key": "tag_value",
            "message": "log from vector",
        }]
    );
}

#[test]
fn generates_log_api_model_with_message_field() {
    let mut map = HashMap::<KeyString, Value>::new();
    map.insert("tag_key".into(), Value::from("tag_value".to_owned()));
    map.insert(
        "message".into(),
        Value::from("This is a message".to_owned()),
    );
    let event = Event::Log(LogEvent::from(map));
    let model = LogsApiModel::try_from(vec![event]).expect("Failed mapping logs into API model");
    let logs = model.0[0].get("logs").expect("Logs data store not present");

    assert_eq!(
        &logs[..],
        &[model_map! {
            "tag_key": "tag_value",
            "message": "This is a message",
        }]
    );
}

#[test]
fn generates_metric_api_model_without_timestamp() {
    let event = Event::Metric(Metric::new(
        "my_metric",
        MetricKind::Absolute,
        MetricValue::Counter { value: 100.0 },
    ));
    let model =
        MetricsApiModel::try_from(vec![event]).expect("Failed mapping metrics into API model");
    let metrics = model.0[0]
        .get("metrics")
        .expect("Metric data store not present");

    assert_eq!(
        &metrics[..],
        &[model_map! {
            "name": "my_metric",
            "value": 100.0,
            "timestamp": metrics[0].get("timestamp").unwrap().clone(),
            "type": "gauge",
        }]
    );
}

#[test]
fn generates_metric_api_model_with_timestamp() {
    let stamp = DateTime::<Utc>::from(SystemTime::now());
    let m = Metric::new(
        "my_metric",
        MetricKind::Absolute,
        MetricValue::Counter { value: 100.0 },
    )
    .with_timestamp(Some(stamp));
    let event = Event::Metric(m);
    let model =
        MetricsApiModel::try_from(vec![event]).expect("Failed mapping metrics into API model");
    let metrics = model.0[0]
        .get("metrics")
        .expect("Metric data store not present");

    assert_eq!(
        &metrics[..],
        &[model_map! {
            "name": "my_metric",
            "value": 100.0,
            "timestamp": stamp.timestamp(),
            "type": "gauge",
        }]
    );
}

#[test]
fn generates_metric_api_model_incremental_counter() {
    let stamp = DateTime::<Utc>::from(SystemTime::now());
    let m = Metric::new(
        "my_metric",
        MetricKind::Incremental,
        MetricValue::Counter { value: 100.0 },
    )
    .with_timestamp(Some(stamp))
    .with_interval_ms(NonZeroU32::new(1000));
    let event = Event::Metric(m);
    let model =
        MetricsApiModel::try_from(vec![event]).expect("Failed mapping metrics into API model");
    let metrics = model.0[0]
        .get("metrics")
        .expect("Metric data store not present");

    assert_eq!(
        &metrics[..],
        &[model_map! {
            "name": "my_metric",
            "value": 100.0,
            "interval.ms": 1000,
            "timestamp": stamp.timestamp(),
            "type": "count",
        }]
    );
}
