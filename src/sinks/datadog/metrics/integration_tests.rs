use bytes::Bytes;
use flate2::read::ZlibDecoder;
use futures::{channel::mpsc::Receiver, stream, StreamExt};
use hyper::StatusCode;
use indoc::indoc;
use rand::{thread_rng, Rng};
use vector_core::event::{BatchNotifier, BatchStatus, Event, Metric, MetricKind, MetricValue};

use super::DatadogMetricsConfig;
use crate::{
    config::SinkConfig,
    sinks::util::test::{build_test_server_status, load_sink},
    test_util::{
        components::{assert_sink_compliance, SINK_TAGS},
        map_event_batch_stream, next_addr,
    },
};

enum ApiStatus {
    OK,
    // Forbidden,
}

fn test_server(
    addr: std::net::SocketAddr,
    api_status: ApiStatus,
) -> (
    futures::channel::mpsc::Receiver<(http::request::Parts, Bytes)>,
    stream_cancel::Trigger,
    impl std::future::Future<Output = Result<(), ()>>,
) {
    let status = match api_status {
        ApiStatus::OK => StatusCode::OK,
        // ApiStatus::Forbidden => StatusCode::FORBIDDEN,
    };

    // NOTE: we pass `Trigger` out to the caller even though this suite never
    // uses it as it's being dropped cancels the stream machinery here,
    // indicating failures that might not be valid.
    build_test_server_status(addr, status)
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
async fn start_test(
    api_status: ApiStatus,
    batch_status: BatchStatus,
) -> (Vec<Event>, Receiver<(http::request::Parts, Bytes)>) {
    let config = indoc! {r#"
        default_api_key = "atoken"
        default_namespace = "foo"
    "#};
    let (mut config, cx) = load_sink::<DatadogMetricsConfig>(config).unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it
    // to our local server
    let endpoint = format!("http://{}", addr);
    config.endpoint = Some(endpoint.clone());

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = test_server(addr, api_status);
    tokio::spawn(server);

    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let events: Vec<_> = (0..10)
        .map(|index| {
            Event::Metric(Metric::new(
                format!("counter_{}", thread_rng().gen::<u32>()),
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: index as f64,
                },
            ))
        })
        .collect();
    let stream = map_event_batch_stream(stream::iter(events.clone()), Some(batch));

    sink.run(stream).await.unwrap();
    assert_eq!(receiver.await, batch_status);

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
    let (expected, rx) = start_test(ApiStatus::OK, BatchStatus::Delivered).await;

    let output = rx.take(expected.len()).collect::<Vec<_>>().await;

    for val in output.iter() {
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
        assert_eq!(
            entry.get("metric").unwrap().as_str().unwrap(),
            "foo.counter"
        );
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
        assert_eq!(points.get(1).unwrap().as_f64().unwrap(), 1.0);
    }
}

#[tokio::test]
async fn real_endpoint() {
    assert_sink_compliance(&SINK_TAGS, async {
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
