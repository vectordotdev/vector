use bytes::Bytes;
use futures::{channel::mpsc::Receiver, stream, StreamExt};
use http::header::AUTHORIZATION;
use hyper::StatusCode;
use indoc::indoc;
use vector_lib::event::{
    BatchNotifier, BatchStatus, Event, LogEvent, Metric, MetricKind, MetricValue,
};

use crate::{
    config::SinkConfig,
    sinks::appsignal::config::AppsignalConfig,
    sinks::util::test::{build_test_server_status, load_sink},
    test_util::{
        components::{
            assert_sink_compliance, assert_sink_error, run_and_assert_sink_compliance,
            COMPONENT_ERROR_TAGS, HTTP_SINK_TAGS,
        },
        generate_lines_with_stream, map_event_batch_stream, next_addr,
    },
};

async fn start_test(events: Vec<Event>) -> (Vec<Event>, Receiver<(http::request::Parts, Bytes)>) {
    let config = indoc! {r#"
        push_api_key = "${TEST_APPSIGNAL_PUSH_API_KEY}"
        compression = "none"
    "#};
    let config = config.replace("${TEST_APPSIGNAL_PUSH_API_KEY}", &push_api_key());
    let (mut config, cx) = load_sink::<AppsignalConfig>(config.as_str()).unwrap();
    let addr = next_addr();
    // Set the endpoint to a local server so we can fetch the sent events later
    config.endpoint = format!("http://{}", addr);

    let (sink, _) = config.build(cx).await.unwrap();

    // Always return OK from server. We're not testing responses.
    let (rx, _trigger, server) = build_test_server_status(addr, StatusCode::OK);
    tokio::spawn(server);

    let (batch, receiver) = BatchNotifier::new_with_receiver();

    let stream = map_event_batch_stream(stream::iter(events.clone()), Some(batch));

    sink.run(stream).await.unwrap();
    assert_eq!(receiver.await, BatchStatus::Delivered);

    (events, rx)
}

#[tokio::test]
async fn logs_real_endpoint() {
    let config = indoc! {r#"
        push_api_key = "${TEST_APPSIGNAL_PUSH_API_KEY}"
    "#};
    let config = config.replace("${TEST_APPSIGNAL_PUSH_API_KEY}", &push_api_key());
    let (config, cx) = load_sink::<AppsignalConfig>(config.as_str()).unwrap();

    let (sink, _) = config.build(cx).await.unwrap();
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let generator = |index| format!("this is a log with index {}", index);
    let (_, events) = generate_lines_with_stream(generator, 10, Some(batch));

    run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

    assert_eq!(receiver.await, BatchStatus::Delivered);
}

#[tokio::test]
async fn metrics_real_endpoint() {
    assert_sink_compliance(&HTTP_SINK_TAGS, async {
        let config = indoc! {r#"
            push_api_key = "${TEST_APPSIGNAL_PUSH_API_KEY}"
        "#};
        let config = config.replace("${TEST_APPSIGNAL_PUSH_API_KEY}", &push_api_key());
        let (config, cx) = load_sink::<AppsignalConfig>(config.as_str()).unwrap();

        let (sink, _) = config.build(cx).await.unwrap();
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let events: Vec<_> = (0..10)
            .map(|index| {
                Event::Metric(Metric::new(
                    "counter",
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: index as f64,
                    },
                ))
            })
            .collect();
        let stream = map_event_batch_stream(stream::iter(events.clone()), Some(batch));

        sink.run(stream).await.unwrap();
        assert_eq!(receiver.await, BatchStatus::Delivered);
    })
    .await;
}

#[tokio::test]
async fn metrics_shape() {
    let events: Vec<_> = (0..5)
        .flat_map(|index| {
            vec![
                Event::Metric(Metric::new(
                    format!("counter_{}", index),
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: index as f64,
                    },
                )),
                Event::Metric(Metric::new(
                    format!("counter_{}", index),
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: (index + index) as f64,
                    },
                )),
            ]
        })
        .collect();
    let api_key = push_api_key();
    let (expected, rx) = start_test(events).await;
    let output = rx.take(expected.len()).collect::<Vec<_>>().await;

    for val in output.iter() {
        assert_eq!(
            val.0.headers.get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(
            val.0.headers.get(AUTHORIZATION).unwrap(),
            &format!("Bearer {api_key}")
        );

        let payload = std::str::from_utf8(&val.1).unwrap();
        let payload: serde_json::Value = serde_json::from_str(payload).unwrap();
        let events = payload.as_array().unwrap();
        assert_eq!(events.len(), 5);

        let metrics: Vec<(&str, &str, f64)> = events
            .iter()
            .map(|json_value| {
                let metric = json_value
                    .as_object()
                    .unwrap()
                    .get("metric")
                    .unwrap()
                    .as_object()
                    .unwrap();
                let name = metric.get("name").unwrap().as_str().unwrap();
                let kind = metric.get("kind").unwrap().as_str().unwrap();
                let counter = metric.get("counter").unwrap().as_object().unwrap();
                let value = counter.get("value").unwrap().as_f64().unwrap();
                (name, kind, value)
            })
            .collect();
        assert_eq!(
            vec![
                ("counter_0", "incremental", 0.0),
                ("counter_1", "incremental", 1.0),
                ("counter_2", "incremental", 2.0),
                ("counter_3", "incremental", 3.0),
                ("counter_4", "incremental", 4.0),
            ],
            metrics
        );
    }
}

#[tokio::test]
async fn logs_shape() {
    let events: Vec<_> = (0..5)
        .map(|index| Event::Log(LogEvent::from(format!("Log message {index}"))))
        .collect();
    let api_key = push_api_key();
    let (expected, rx) = start_test(events).await;
    let output = rx.take(expected.len()).collect::<Vec<_>>().await;

    for val in output.iter() {
        assert_eq!(
            val.0.headers.get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(
            val.0.headers.get(AUTHORIZATION).unwrap(),
            &format!("Bearer {api_key}")
        );

        let payload = std::str::from_utf8(&val.1).unwrap();
        let payload: serde_json::Value = serde_json::from_str(payload).unwrap();
        let events = payload.as_array().unwrap();
        assert_eq!(events.len(), 5);

        let log_messages: Vec<&str> = events
            .iter()
            .map(|value| {
                value
                    .as_object()
                    .unwrap()
                    .get("log")
                    .unwrap()
                    .as_object()
                    .unwrap()
                    .get("message")
                    .unwrap()
                    .as_str()
                    .unwrap()
            })
            .collect();
        assert_eq!(
            vec![
                "Log message 0",
                "Log message 1",
                "Log message 2",
                "Log message 3",
                "Log message 4",
            ],
            log_messages
        );

        let event = events
            .last()
            .unwrap()
            .as_object()
            .unwrap()
            .get("log")
            .unwrap()
            .as_object()
            .unwrap();
        assert!(!event.get("timestamp").unwrap().as_str().unwrap().is_empty());
    }
}

#[tokio::test]
async fn error_scenario_real_endpoint() {
    assert_sink_error(&COMPONENT_ERROR_TAGS, async {
        let config = indoc! {r#"
            push_api_key = "invalid key"
        "#};
        let (config, cx) = load_sink::<AppsignalConfig>(config).unwrap();

        let (sink, _) = config.build(cx).await.unwrap();
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let events = vec![
            Event::Metric(Metric::new(
                "counter",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            )),
            Event::Metric(Metric::new(
                "counter",
                MetricKind::Absolute,
                MetricValue::Counter { value: 2.0 },
            )),
        ];
        let stream = map_event_batch_stream(stream::iter(events.clone()), Some(batch));

        sink.run(stream).await.unwrap();
        assert_eq!(receiver.await, BatchStatus::Rejected);
    })
    .await;
}

fn push_api_key() -> String {
    let api_key = std::env::var("TEST_APPSIGNAL_PUSH_API_KEY")
        .expect("couldn't find the AppSignal push API key in environment variables");
    assert!(!api_key.is_empty(), "$TEST_APPSIGNAL_PUSH_API_KEY required");
    api_key
}
