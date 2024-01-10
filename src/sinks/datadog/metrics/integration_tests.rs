use std::num::NonZeroU32;

use bytes::Bytes;
use chrono::{SubsecRound, Utc};
use flate2::read::ZlibDecoder;
use futures::{channel::mpsc::Receiver, stream, StreamExt};
use http::request::Parts;
use hyper::StatusCode;
use indoc::indoc;
use prost::Message;
use rand::{thread_rng, Rng};

use vector_lib::{
    config::{init_telemetry, Tags, Telemetry},
    event::{BatchNotifier, BatchStatus, Event, Metric, MetricKind, MetricValue},
    metric_tags,
};

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

use super::{
    config::{SERIES_V1_PATH, SERIES_V2_PATH},
    encoder::{ORIGIN_CATEGORY_VALUE, ORIGIN_PRODUCT_VALUE},
    DatadogMetricsConfig,
};

#[allow(warnings, clippy::pedantic, clippy::nursery)]
mod ddmetric_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

fn generate_counters() -> Vec<Event> {
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
                .with_timestamp(Some(ts))
                .with_tags(Some(metric_tags!(
                    "resource.device" => "a_device",
                    "host" => "a_host",
                    "source_type_name" => "a_name",
                    "cool_tag_name" => "i_know_right",
                ))),
            )
            // this ensures we get Origin Metadata, with an undefined service but that's ok.
            .with_source_type("a_source_like_none_other")
        })
        .collect();

    events
}

fn generate_counter_gauge_set() -> Vec<Event> {
    let ts = Utc::now().trunc_subsecs(3);
    let events = vec![
        // gauge
        Event::Metric(
            Metric::new(
                "gauge",
                MetricKind::Incremental,
                MetricValue::Gauge { value: 5678.0 },
            )
            // Dogstatsd outputs gauges with an interval
            .with_interval_ms(NonZeroU32::new(10000)),
        ),
        // counter with interval
        Event::Metric(
            Metric::new(
                "counter_with_interval",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1234.0 },
            )
            .with_interval_ms(NonZeroU32::new(2000))
            .with_timestamp(Some(ts)),
        ),
        // set
        Event::Metric(Metric::new(
            "set",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["zorp".into(), "zork".into()].into_iter().collect(),
            },
        )),
    ];

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
async fn start_test(events: Vec<Event>) -> (Vec<Event>, Receiver<(http::request::Parts, Bytes)>) {
    let config = indoc! {r#"
        default_api_key = "atoken"
        default_namespace = "foo"
    "#};
    let (mut config, cx) = load_sink::<DatadogMetricsConfig>(config).unwrap();

    let addr = next_addr();
    // Swap out the endpoint so we can force send it
    // to our local server
    let endpoint = format!("http://{}", addr);
    config.local_dd_common.endpoint = Some(endpoint.clone());

    let (sink, _) = config.build(cx).await.unwrap();

    let (rx, _trigger, server) = build_test_server_status(addr, StatusCode::OK);
    tokio::spawn(server);

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();

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
/// Assert proper handling of different metric types
async fn all_series_metric_types() {
    let metrics = generate_counter_gauge_set();
    let (expected, rx) = start_test(metrics).await;

    let output = rx.take(expected.len()).collect::<Vec<_>>().await;

    assert!(output.len() == 1, "Should have received a response");

    let request = output.first().unwrap();

    match request.0.uri.path() {
        SERIES_V1_PATH => warn!("Deprecated endpoint used."),
        SERIES_V2_PATH => validate_protobuf_set_gauge_rate(request),
        _ => panic!("Unexpected request type received!"),
    }
}

#[tokio::test]
/// Assert the basic functionality of the sink in good conditions with
/// a small batch of counters.
///
/// This test rigs the sink to return OK to responses, checks that all batches
/// were delivered and then asserts that every message is able to be
/// deserialized.
///
/// In addition to validating the counter values, we also validate the various
/// fields such as the Resources, handling of tags, and the Metadata.
async fn smoke() {
    let counters = generate_counters();
    let (expected, rx) = start_test(counters).await;

    let output = rx.take(expected.len()).collect::<Vec<_>>().await;

    assert!(output.len() == 1, "Should have received a response");

    let request = output.first().unwrap();

    match request.0.uri.path() {
        SERIES_V1_PATH => validate_json_counters(request),
        SERIES_V2_PATH => validate_protobuf_counters(request),
        _ => panic!("Unexpected request type received!"),
    }
}

fn validate_common(request: &(Parts, Bytes)) {
    assert_eq!(request.0.headers.get("DD-API-KEY").unwrap(), "atoken");
    assert!(request.0.headers.contains_key("DD-Agent-Payload"));
}

fn validate_protobuf_counters(request: &(Parts, Bytes)) {
    assert_eq!(
        request.0.headers.get("Content-Type").unwrap(),
        "application/x-protobuf"
    );

    validate_common(request);

    let compressed_payload = request.1.to_vec();
    let payload = decompress_payload(compressed_payload).expect("Could not decompress payload");
    let frame = Bytes::copy_from_slice(&payload);

    let payload =
        ddmetric_proto::MetricPayload::decode(frame).expect("Could not decode protobuf frame");

    let series = payload.series;

    assert!(!series.is_empty());

    // check metrics are sorted by name, which helps HTTP compression
    let metric_names: Vec<String> = series.iter().map(|serie| serie.metric.clone()).collect();
    let mut sorted_names = metric_names.clone();
    sorted_names.sort();
    assert_eq!(metric_names, sorted_names);

    series.iter().for_each(|serie| {
        // name
        assert!(serie.metric.starts_with("foo.counter_"));

        // type
        assert_eq!(
            serie.r#type(),
            ddmetric_proto::metric_payload::MetricType::Count
        );

        // resources
        serie
            .resources
            .iter()
            .for_each(|resource| match resource.r#type.as_str() {
                "host" => assert_eq!(resource.name.as_str(), "a_host"),
                "device" => assert_eq!(resource.name.as_str(), "a_device"),
                _ => panic!("Unexpected resource found!"),
            });

        // source_type_name
        assert_eq!(serie.source_type_name, "a_name");

        // tags
        assert_eq!(serie.tags.len(), 1);
        assert_eq!(serie.tags.first().unwrap(), "cool_tag_name:i_know_right");

        // unit
        assert!(serie.unit.is_empty());

        // interval
        assert_eq!(serie.interval, 0);

        // metadata
        let origin_metadata = serie.metadata.as_ref().unwrap().origin.as_ref().unwrap();
        assert_eq!(origin_metadata.origin_product, *ORIGIN_PRODUCT_VALUE);
        assert_eq!(origin_metadata.origin_category, ORIGIN_CATEGORY_VALUE);
        assert_eq!(origin_metadata.origin_service, 0);
    });

    // points
    // the input values are [0..10)
    assert_eq!(
        series
            .iter()
            .map(|serie| serie.points.iter().map(|point| point.value).sum::<f64>())
            .sum::<f64>(),
        45.0
    );
}

fn validate_protobuf_set_gauge_rate(request: &(Parts, Bytes)) {
    assert_eq!(
        request.0.headers.get("Content-Type").unwrap(),
        "application/x-protobuf"
    );

    validate_common(request);

    let compressed_payload = request.1.to_vec();
    let payload = decompress_payload(compressed_payload).expect("Could not decompress payload");
    let frame = Bytes::copy_from_slice(&payload);

    let payload =
        ddmetric_proto::MetricPayload::decode(frame).expect("Could not decode protobuf frame");

    let mut series = payload.series;

    assert_eq!(series.len(), 3);

    // The below evaluation of each metric type implies validation of sorting the metrics
    // by name to improve HTTP compression due to the order they are defined vs processed.
    // However just to be safe we will also validate explicitly.
    let metric_names: Vec<String> = series.iter().map(|serie| serie.metric.clone()).collect();
    let mut sorted_names = metric_names.clone();
    sorted_names.sort();
    assert_eq!(metric_names, sorted_names);

    // validate set (gauge)
    {
        let gauge = series.pop().unwrap();
        assert_eq!(
            gauge.r#type(),
            ddmetric_proto::metric_payload::MetricType::Gauge
        );
        assert_eq!(gauge.interval, 0);
        assert_eq!(gauge.points[0].value, 2_f64);
    }

    // validate gauge
    {
        let gauge = series.pop().unwrap();
        assert_eq!(
            gauge.r#type(),
            ddmetric_proto::metric_payload::MetricType::Gauge
        );
        assert_eq!(gauge.points[0].value, 5678.0);
        assert_eq!(gauge.interval, 10);
    }

    // validate counter w interval = rate
    {
        let count = series.pop().unwrap();
        assert_eq!(
            count.r#type(),
            ddmetric_proto::metric_payload::MetricType::Rate
        );
        assert_eq!(count.interval, 2);

        assert_eq!(count.points.len(), 1);
        assert_eq!(count.points[0].value, 1234.0 / count.interval as f64);
    }
}

fn validate_json_counters(request: &(Parts, Bytes)) {
    assert_eq!(
        request.0.headers.get("Content-Type").unwrap(),
        "application/json"
    );

    validate_common(request);

    let compressed_payload = request.1.to_vec();
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

    let events = generate_counters();

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
