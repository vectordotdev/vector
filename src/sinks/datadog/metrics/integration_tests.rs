use bytes::Bytes;
use chrono::{SubsecRound, Utc};
use flate2::read::ZlibDecoder;
use futures::{channel::mpsc::Receiver, stream, StreamExt};
use hyper::StatusCode;
use indoc::indoc;
use rand::{thread_rng, Rng};
use vector_core::{
    config::{init_telemetry, Tags, Telemetry},
    event::{BatchNotifier, BatchStatus, Event, Metric, MetricKind, MetricValue},
};

use super::DatadogMetricsConfig;
use crate::{
    config::SinkConfig,
    sinks::util::test::{build_test_server_status, load_sink},
    test_util::{
        components::{
            assert_data_volume_sink_compliance, assert_sink_compliance, DATA_VOLUME_SINK_TAGS,
            SINK_TAGS,
        },
        map_event_batch_stream, next_addr,
    },
};

fn generate_metric_events() -> Vec<Event> {
    let timestamp = Utc::now().trunc_subsecs(3);
    let events: Vec<_> = (0..10)
        .map(|index| {
            let ts = timestamp + (std::time::Duration::from_secs(2) * index);
            Event::Metric(
                Metric::new(
                    format!("counter_{}", thread_rng().gen::<u32>()),
                    MetricKind::Incremental,
                    MetricValue::Counter {
                        value: index as f64,
                    },
                )
                .with_timestamp(Some(ts)),
            )
        })
        .collect();

    events
}

/// Starts a test sink with random metrics running into it
///
/// This function starts a Datadog Metrics sink with a simplistic configuration and
/// runs random lines through it, returning a vector of the random lines and a
/// Receiver populated with the result of the sink's operation.
///
/// Testers may set `http_status` and `batch_status`. The first controls what
/// status code faked HTTP responses will have, the second acts as a check on
/// the `Receiver`'s status before being returned to the caller.
async fn start_test() -> (Vec<Event>, Receiver<(http::request::Parts, Bytes)>) {
    let config = indoc! {r#"
        default_api_key = "atoken"
        default_namespace = "foo"
    "#};
    let (mut config, cx) = load_sink::<DatadogMetricsConfig>(config).unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it
    // to our local server
    let endpoint = format!("http://{}", addr);
    config.dd_common.endpoint = Some(endpoint.clone());

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = build_test_server_status(addr, StatusCode::OK);
    tokio::spawn(server);

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();

    let events = generate_metric_events();

    let stream = map_event_batch_stream(stream::iter(events.clone()), Some(batch));

    sink.run(stream).await.unwrap();

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    (events, rx)
}

fn decompress_payload(payload: Vec<u8>) -> std::io::Result<Vec<u8>> {
    let mut decompressor = ZlibDecoder::new(&payload[..]);
    let mut decompressed = Vec::new();
    let result = std::io::copy(&mut decompressor, &mut decompressed);
    result.map(|_| decompressed)
}

#[tokio::test]
/// Assert the basic functionality of the sink in good conditions
///
/// This test rigs the sink to return OK to responses, checks that all batches
/// were delivered and then asserts that every message is able to be
/// deserialized.
async fn smoke() {
    let (expected, rx) = start_test().await;

    let output = rx.take(expected.len()).collect::<Vec<_>>().await;

    assert!(output.len() == 1);

    output.first().map(|val| {
        assert_eq!(
            val.0.headers.get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(val.0.headers.get("DD-API-KEY").unwrap(), "atoken");
        assert!(val.0.headers.contains_key("DD-Agent-Payload"));

        let compressed_payload = val.1.to_vec();
        let payload = decompress_payload(compressed_payload).unwrap();
        let payload = std::str::from_utf8(&payload).unwrap();
        let payload: serde_json::Value = serde_json::from_str(payload).unwrap();

        let series = payload
            .as_object()
            .unwrap()
            .get("series")
            .unwrap()
            .as_array()
            .unwrap();
        assert!(!series.is_empty());

        // check metrics are sorted by name, which helps HTTP compression
        let metric_names: Vec<String> = series
            .iter()
            .map(|value| {
                value
                    .as_object()
                    .unwrap()
                    .get("metric")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        let mut sorted_names = metric_names.clone();
        sorted_names.sort();
        assert_eq!(metric_names, sorted_names);

        let entry = series.first().unwrap().as_object().unwrap();
        assert!(entry
            .get("metric")
            .unwrap()
            .as_str()
            .unwrap()
            .starts_with("foo.counter_"),);
        assert_eq!(entry.get("type").unwrap().as_str().unwrap(), "count");
        let points = entry
            .get("points")
            .unwrap()
            .as_array()
            .unwrap()
            .first()
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(points.len(), 2);

        // validate that all values were received
        let all_values: f64 = series
            .iter()
            .map(|entry| {
                entry
                    .as_object()
                    .unwrap()
                    .get("points")
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .get(1)
                    .unwrap()
                    .as_f64()
                    .unwrap()
            })
            .sum();

        // the input values are [0..10)
        assert_eq!(all_values, 45.0);
    });
}

async fn run_sink() {
    let config = indoc! {r#"
        default_api_key = "${TEST_DATADOG_API_KEY}"
        default_namespace = "fake.test.integration"
    "#};
    let api_key = std::env::var("TEST_DATADOG_API_KEY").unwrap();
    assert!(!api_key.is_empty(), "$TEST_DATADOG_API_KEY required");
    let config = config.replace("${TEST_DATADOG_API_KEY}", &api_key);
    let (config, cx) = load_sink::<DatadogMetricsConfig>(config.as_str()).unwrap();

    let (sink, _) = config.build(cx).await.unwrap();
    let (batch, receiver) = BatchNotifier::new_with_receiver();

    let events = generate_metric_events();

    let stream = map_event_batch_stream(stream::iter(events.clone()), Some(batch));

    sink.run(stream).await.unwrap();
    assert_eq!(receiver.await, BatchStatus::Delivered);
}

#[tokio::test]
async fn real_endpoint() {
    assert_sink_compliance(&SINK_TAGS, async { run_sink().await }).await;
}

#[tokio::test]
async fn data_volume_tags() {
    init_telemetry(
        Telemetry {
            tags: Tags {
                emit_service: true,
                emit_source: true,
            },
        },
        true,
    );

    assert_data_volume_sink_compliance(&DATA_VOLUME_SINK_TAGS, async { run_sink().await }).await;
}
