use std::{collections::HashMap, io, time::Duration};

use futures::stream;
use tower::ServiceExt;
use vector_lib::{
    codecs::{JsonSerializerConfig, TextSerializerConfig},
    event::{BatchNotifier, BatchStatus, LogEvent, Metric, MetricKind, MetricValue},
    request_metadata::GroupedCountByteSize,
    stream::Driver,
};

use super::{
    RedisDelivery, RedisEvent, RedisRequest,
    config::RedisSinkConfig,
    request_builder::{encode_event, request_builder},
    service::{RedisResponse, RedisResponseService},
};
use crate::{
    codecs::{Encoder, Transformer},
    config::log_schema,
    metrics::{self, Controller},
    sinks::prelude::DriverResponse,
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<RedisSinkConfig>();
}

#[test]
fn redis_retry_clones_share_confirmed_delivery() {
    let delivery = RedisDelivery::new(2);
    let retry = delivery.clone();
    assert_eq!(delivery.record(&[true, false]), [true, false]);
    assert_eq!(retry.record(&[false, false]), [true, false]);
    assert_eq!(retry.record(&[false, true]), [true, true]);
    assert_eq!(delivery.snapshot(), [true, true]);
    assert_eq!(RedisDelivery::new(2).snapshot(), [false, false]);
}

#[tokio::test]
async fn redis_terminal_transport_errors_preserve_delivery() {
    for confirmed in [0, 1] {
        assert_terminal_redis_error(confirmed, false).await;
    }
}

#[tokio::test]
async fn redis_terminal_timeouts_preserve_delivery() {
    for confirmed in [0, 1] {
        assert_terminal_redis_error(confirmed, true).await;
    }
}

async fn assert_terminal_redis_error(confirmed: usize, timeout: bool) {
    metrics::init_test();
    let controller = Controller::get().unwrap();
    controller.reset();
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let events = ["first", "second"]
        .into_iter()
        .map(|message| RedisEvent {
            event: LogEvent::from(message).with_batch_notifier(&batch).into(),
            key: message.to_owned(),
            score: None,
        })
        .collect();
    drop(batch);
    let request = request_builder(
        events,
        &Transformer::default(),
        &mut Encoder::<()>::new(TextSerializerConfig::default().build().into()),
    );
    request.delivery.record(&[confirmed == 1, false]);
    let service = tower::service_fn(move |_: RedisRequest| async move {
        if timeout {
            std::future::pending::<()>().await;
        }
        Err::<RedisResponse, crate::Error>(
            io::Error::new(io::ErrorKind::ConnectionReset, "connection reset").into(),
        )
    });
    let service = tower::timeout::Timeout::new(service, Duration::from_millis(1));
    let service =
        RedisResponseService::new(service).map_response(move |response: RedisResponse| {
            let error = response.error.as_ref().unwrap();
            if timeout {
                assert!(error.is::<tower::timeout::error::Elapsed>());
            } else {
                assert_eq!(
                    error.downcast_ref::<io::Error>().unwrap().kind(),
                    io::ErrorKind::ConnectionReset
                );
            }
            assert_eq!(response.events_dropped(), Some(2 - confirmed));
            assert_eq!(
                format!("{:?}", response.error().unwrap()),
                format!("{error:?}")
            );
            response
        });
    Driver::new(stream::iter([request]), service)
        .run()
        .await
        .unwrap();
    assert_eq!(receiver.await, BatchStatus::Rejected);

    let metrics = controller.capture_metrics();
    for (name, expected) in [
        (
            "component_discarded_events_total",
            if confirmed == 0 { 2.0 } else { 1.0 },
        ),
        ("component_errors_total", 1.0),
        ("component_sent_events_total", 0.0),
    ] {
        let count: f64 = metrics
            .iter()
            .filter(|metric| metric.name() == name)
            .map(|metric| match metric.value() {
                MetricValue::Counter { value } => *value,
                _ => panic!("{name} must be a counter"),
            })
            .sum();
        assert_eq!(
            MetricValue::Counter { value: count },
            MetricValue::Counter { value: expected },
            "timeout={timeout}, confirmed={confirmed}: {name}"
        );
    }
}

#[tokio::test]
async fn redis_terminal_outcome_metrics() {
    metrics::init_test();
    let controller = Controller::get().unwrap();
    for (event_status, expected_discarded, expected_batch_status) in [
        ([true, false], 1.0, BatchStatus::Errored),
        ([false, true], 1.0, BatchStatus::Errored),
        ([false, false], 2.0, BatchStatus::Errored),
        ([true, true], 0.0, BatchStatus::Delivered),
    ] {
        controller.reset();
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let events = ["first", "second"]
            .into_iter()
            .map(|message| RedisEvent {
                event: LogEvent::from(message).with_batch_notifier(&batch).into(),
                key: message.to_owned(),
                score: None,
            })
            .collect();
        drop(batch);
        let request = request_builder(
            events,
            &Transformer::default(),
            &mut Encoder::<()>::new(TextSerializerConfig::default().build().into()),
        );
        let service = tower::service_fn(move |request: RedisRequest| {
            let byte_size = request.metadata.events_byte_size();
            std::future::ready(Ok::<_, crate::Error>(RedisResponse {
                event_status: event_status.to_vec(),
                events_byte_size: request
                    .metadata
                    .into_events_estimated_json_encoded_byte_size(),
                byte_size,
                error: None,
            }))
        });

        Driver::new(stream::iter([request]), service)
            .run()
            .await
            .unwrap();
        assert_eq!(receiver.await, expected_batch_status, "{event_status:?}");

        let metrics = controller.capture_metrics();
        let delivered = expected_batch_status == BatchStatus::Delivered;
        for (name, expected) in [
            ("component_discarded_events_total", expected_discarded),
            ("component_errors_total", if delivered { 0.0 } else { 1.0 }),
            (
                "component_sent_events_total",
                if delivered { 2.0 } else { 0.0 },
            ),
        ] {
            let count: f64 = metrics
                .iter()
                .filter(|metric| metric.name() == name)
                .map(|metric| {
                    let MetricValue::Counter { value } = metric.value() else {
                        panic!("{name} must be a counter");
                    };
                    *value
                })
                .sum();
            assert_eq!(count, expected, "{event_status:?}: {name}");
        }
    }
}

#[test]
fn redis_log_event_json() {
    let msg = "hello_world".to_owned();
    let mut byte_size = GroupedCountByteSize::new_untagged();
    let mut evt = LogEvent::from(msg.clone());
    evt.insert(vrl::event_path!("key"), "value");
    let result = encode_event(
        evt.into(),
        "key".to_string(),
        None,
        &Transformer::default(),
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
        &Transformer::default(),
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
    evt.insert(vrl::event_path!("key"), "value");

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
        &Transformer::default(),
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
    evt.insert(vrl::event_path!("key"), "value");

    let result = encode_event(
        evt.into(),
        "key".to_string(),
        Some(64),
        &Transformer::default(),
        &mut Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
        &mut byte_size,
    )
    .unwrap()
    .score;

    assert_eq!(result, Some(64));
}
