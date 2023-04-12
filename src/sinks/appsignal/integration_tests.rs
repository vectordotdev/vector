use futures::stream;
use indoc::indoc;
use vector_core::event::{BatchNotifier, BatchStatus, Event, Metric, MetricKind, MetricValue};

use crate::{
    config::SinkConfig,
    sinks::appsignal::AppsignalSinkConfig,
    sinks::util::test::load_sink,
    test_util::{
        components::{
            assert_sink_compliance, assert_sink_error, run_and_assert_sink_compliance,
            COMPONENT_ERROR_TAGS, SINK_TAGS,
        },
        generate_lines_with_stream, map_event_batch_stream,
    },
};

#[tokio::test]
async fn logs_real_endpoint() {
    let config = indoc! {r#"
        push_api_key = "${TEST_APPSIGNAL_PUSH_API_KEY}"
    "#};
    let api_key = std::env::var("TEST_APPSIGNAL_PUSH_API_KEY")
        .expect("couldn't find the AppSignal push API key in environment variables");
    assert!(!api_key.is_empty(), "$TEST_APPSIGNAL_PUSH_API_KEY required");
    let config = config.replace("${TEST_APPSIGNAL_PUSH_API_KEY}", &api_key);
    let (config, cx) = load_sink::<AppsignalSinkConfig>(config.as_str()).unwrap();

    let (sink, _) = config.build(cx).await.unwrap();
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let generator = |index| format!("this is a log with index {}", index);
    let (_, events) = generate_lines_with_stream(generator, 10, Some(batch));

    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;

    assert_eq!(receiver.await, BatchStatus::Delivered);
}

#[tokio::test]
async fn metrics_real_endpoint() {
    assert_sink_compliance(&SINK_TAGS, async {
        let config = indoc! {r#"
            push_api_key = "${TEST_APPSIGNAL_PUSH_API_KEY}"
        "#};
        let api_key = std::env::var("TEST_APPSIGNAL_PUSH_API_KEY")
            .expect("couldn't find the AppSignal push API key in environment variables");
        assert!(!api_key.is_empty(), "$TEST_APPSIGNAL_PUSH_API_KEY required");
        let config = config.replace("${TEST_APPSIGNAL_PUSH_API_KEY}", &api_key);
        let (config, cx) = load_sink::<AppsignalSinkConfig>(config.as_str()).unwrap();

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
async fn error_scenario_real_endpoint() {
    assert_sink_error(&COMPONENT_ERROR_TAGS, async {
        let config = indoc! {r#"
            push_api_key = "invalid key"
        "#};
        let (config, cx) = load_sink::<AppsignalSinkConfig>(config).unwrap();

        let (sink, _) = config.build(cx).await.unwrap();
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let events = vec![Event::Metric(Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        ))];
        let stream = map_event_batch_stream(stream::iter(events.clone()), Some(batch));

        sink.run(stream).await.unwrap();
        assert_eq!(receiver.await, BatchStatus::Rejected);
    })
    .await;
}
