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

#[test]
fn generate_event_api_model() {
    // Without message field
    let mut map = HashMap::<KeyString, Value>::new();
    map.insert("eventType".into(), Value::from("TestEvent".to_owned()));
    map.insert("user".into(), Value::from("Joe".to_owned()));
    map.insert("user_id".into(), Value::from(123456));
    let event = Event::Log(LogEvent::from(map));
    let model =
        EventsApiModel::try_from(vec![event]).expect("Failed mapping events into API model");

    assert_eq!(model.0.len(), 1);
    assert!(model.0[0].get("eventType").is_some());
    assert_eq!(
        model.0[0].get("eventType").unwrap().to_string_lossy(),
        "TestEvent".to_owned()
    );
    assert!(model.0[0].get("user").is_some());
    assert_eq!(
        model.0[0].get("user").unwrap().to_string_lossy(),
        "Joe".to_owned()
    );
    assert!(model.0[0].get("user_id").is_some());
    assert_eq!(model.0[0].get("user_id").unwrap(), &Value::Integer(123456));

    // With message field
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

    assert_eq!(model.0.len(), 1);
    assert!(model.0[0].get("eventType").is_some());
    assert_eq!(
        model.0[0].get("eventType").unwrap().to_string_lossy(),
        "TestEvent".to_owned()
    );
    assert!(model.0[0].get("user").is_some());
    assert_eq!(
        model.0[0].get("user").unwrap().to_string_lossy(),
        "Joe".to_owned()
    );
    assert!(model.0[0].get("user_id").is_some());
    assert_eq!(model.0[0].get("user_id").unwrap(), &Value::Integer(123456));
    assert!(model.0[0].get("message").is_some());
    assert_eq!(
        model.0[0].get("message").unwrap().to_string_lossy(),
        "This is a message".to_owned()
    );

    // With a JSON encoded inside the message field
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

    assert_eq!(model.0.len(), 1);
    assert!(model.0[0].get("eventType").is_some());
    assert_eq!(
        model.0[0].get("eventType").unwrap().to_string_lossy(),
        "TestEvent".to_owned()
    );
    assert!(model.0[0].get("user").is_some());
    assert_eq!(
        model.0[0].get("user").unwrap().to_string_lossy(),
        "Joe".to_owned()
    );
    assert!(model.0[0].get("user_id").is_some());
    assert_eq!(model.0[0].get("user_id").unwrap(), &Value::Integer(123456));
    assert!(model.0[0].get("my_key").is_some());
    assert_eq!(
        model.0[0].get("my_key").unwrap().to_string_lossy(),
        "my_value".to_owned()
    );
}

#[test]
fn generate_log_api_model() {
    // Without message field
    let mut map = HashMap::<KeyString, Value>::new();
    map.insert("tag_key".into(), Value::from("tag_value".to_owned()));
    let event = Event::Log(LogEvent::from(map));
    let model = LogsApiModel::try_from(vec![event]).expect("Failed mapping logs into API model");
    let logs = model.0[0].get("logs").expect("Logs data store not present");

    assert_eq!(logs.len(), 1);
    assert!(logs[0].get("tag_key").is_some());
    assert_eq!(
        logs[0].get("tag_key").unwrap().to_string_lossy(),
        "tag_value".to_owned()
    );
    assert!(logs[0].get("message").is_some());

    // With message field
    let mut map = HashMap::<KeyString, Value>::new();
    map.insert("tag_key".into(), Value::from("tag_value".to_owned()));
    map.insert(
        "message".into(),
        Value::from("This is a message".to_owned()),
    );
    let event = Event::Log(LogEvent::from(map));
    let model = LogsApiModel::try_from(vec![event]).expect("Failed mapping logs into API model");
    let logs = model.0[0].get("logs").expect("Logs data store not present");

    assert_eq!(logs.len(), 1);
    assert!(logs[0].get("tag_key").is_some());
    assert_eq!(
        logs[0].get("tag_key").unwrap().to_string_lossy(),
        "tag_value".to_owned()
    );
    assert!(logs[0].get("message").is_some());
    assert_eq!(
        logs[0].get("message").unwrap().to_string_lossy(),
        "This is a message".to_owned()
    );
}

#[test]
fn generate_metric_api_model() {
    // Without timestamp
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

    assert_eq!(metrics.len(), 1);
    assert!(metrics[0].get("name").is_some());
    assert_eq!(
        metrics[0].get("name").unwrap().to_string_lossy(),
        "my_metric".to_owned()
    );
    assert!(metrics[0].get("value").is_some());
    assert_eq!(metrics[0].get("value").unwrap(), &Value::from(100.0));
    assert!(metrics[0].get("timestamp").is_some());

    // With timestamp
    let m = Metric::new(
        "my_metric",
        MetricKind::Absolute,
        MetricValue::Counter { value: 100.0 },
    )
    .with_timestamp(Some(DateTime::<Utc>::from(SystemTime::now())));
    let event = Event::Metric(m);
    let model =
        MetricsApiModel::try_from(vec![event]).expect("Failed mapping metrics into API model");
    let metrics = model.0[0]
        .get("metrics")
        .expect("Metric data store not present");

    assert_eq!(metrics.len(), 1);
    assert!(metrics[0].get("name").is_some());
    assert_eq!(
        metrics[0].get("name").unwrap().to_string_lossy(),
        "my_metric".to_owned()
    );
    assert!(metrics[0].get("value").is_some());
    assert_eq!(metrics[0].get("value").unwrap(), &Value::from(100.0));
    assert!(metrics[0].get("timestamp").is_some());

    // Incremental counter
    let m = Metric::new(
        "my_metric",
        MetricKind::Incremental,
        MetricValue::Counter { value: 100.0 },
    )
    .with_timestamp(Some(DateTime::<Utc>::from(SystemTime::now())))
    .with_interval_ms(NonZeroU32::new(1000));
    let event = Event::Metric(m);
    let model =
        MetricsApiModel::try_from(vec![event]).expect("Failed mapping metrics into API model");
    let metrics = model.0[0]
        .get("metrics")
        .expect("Metric data store not present");

    assert_eq!(metrics.len(), 1);
    assert!(metrics[0].get("name").is_some());
    assert_eq!(
        metrics[0].get("name").unwrap().to_string_lossy(),
        "my_metric".to_owned()
    );
    assert!(metrics[0].get("value").is_some());
    assert_eq!(metrics[0].get("value").unwrap(), &Value::from(100.0));
    assert!(metrics[0].get("timestamp").is_some());
    assert!(metrics[0].get("interval.ms").is_some());
    assert_eq!(metrics[0].get("interval.ms").unwrap(), &Value::from(1000));
}
