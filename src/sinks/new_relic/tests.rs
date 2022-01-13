use super::*;
use crate::event::{Event, LogEvent, Metric, MetricKind, MetricValue, Value};
use chrono::{DateTime, Utc};
use std::{collections::HashMap, convert::TryFrom, time::SystemTime};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<NewRelicConfig>();
}

#[test]
fn generate_event_api_model() {
    // Without message field
    let mut map = HashMap::<String, Value>::new();
    map.insert("eventType".to_owned(), Value::from("TestEvent".to_owned()));
    map.insert("user".to_owned(), Value::from("Joe".to_owned()));
    map.insert("user_id".to_owned(), Value::from(123456));
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
    let mut map = HashMap::<String, Value>::new();
    map.insert("eventType".to_owned(), Value::from("TestEvent".to_owned()));
    map.insert("user".to_owned(), Value::from("Joe".to_owned()));
    map.insert("user_id".to_owned(), Value::from(123456));
    map.insert(
        "message".to_owned(),
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
    let mut map = HashMap::<String, Value>::new();
    map.insert("eventType".to_owned(), Value::from("TestEvent".to_owned()));
    map.insert("user".to_owned(), Value::from("Joe".to_owned()));
    map.insert("user_id".to_owned(), Value::from(123456));
    map.insert(
        "message".to_owned(),
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
    let mut map = HashMap::<String, Value>::new();
    map.insert("tag_key".to_owned(), Value::from("tag_value".to_owned()));
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
    let mut map = HashMap::<String, Value>::new();
    map.insert("tag_key".to_owned(), Value::from("tag_value".to_owned()));
    map.insert(
        "message".to_owned(),
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
        .expect("Logs data store not present");

    assert_eq!(metrics.len(), 1);
    assert!(metrics[0].get("name").is_some());
    assert_eq!(
        metrics[0].get("name").unwrap().to_string_lossy(),
        "my_metric".to_owned()
    );
    assert!(metrics[0].get("value").is_some());
    assert_eq!(metrics[0].get("value").unwrap(), &Value::Float(100.0));
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
        .expect("Logs data store not present");

    assert_eq!(metrics.len(), 1);
    assert!(metrics[0].get("name").is_some());
    assert_eq!(
        metrics[0].get("name").unwrap().to_string_lossy(),
        "my_metric".to_owned()
    );
    assert!(metrics[0].get("value").is_some());
    assert_eq!(metrics[0].get("value").unwrap(), &Value::Float(100.0));
    assert!(metrics[0].get("timestamp").is_some());
}
