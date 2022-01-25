use super::{DatadogAgentConfig, DatadogAgentSource, DatadogSeriesRequest, LogMsg};
use crate::{
    codecs::{self, BytesDecoder, BytesDeserializer},
    common::datadog::{DatadogMetricType, DatadogPoint, DatadogSeriesMetric},
    config::{log_schema, SourceConfig, SourceContext},
    event::{
        metric::{MetricKind, MetricSketch, MetricValue},
        Event, EventStatus,
    },
    serde::{default_decoding, default_framing_message_based},
    test_util::{
        components::{init_test, COMPONENT_MULTIPLE_OUTPUTS_TESTS},
        next_addr, spawn_collect_n, trace_init, wait_for_tcp,
    },
    SourceSender,
};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::Stream;
use http::HeaderMap;
use pretty_assertions::assert_eq;
use prost::Message;
use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
use std::net::SocketAddr;
use std::str;

mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

impl Arbitrary for LogMsg {
    fn arbitrary(g: &mut Gen) -> Self {
        LogMsg {
            message: Bytes::from(String::arbitrary(g)),
            status: Bytes::from(String::arbitrary(g)),
            timestamp: i64::arbitrary(g),
            hostname: Bytes::from(String::arbitrary(g)),
            service: Bytes::from(String::arbitrary(g)),
            ddsource: Bytes::from(String::arbitrary(g)),
            ddtags: Bytes::from(String::arbitrary(g)),
        }
    }
}

// We want to know that for any json payload that is a `Vec<LogMsg>` we can
// correctly decode it into a `Vec<LogEvent>`. For convenience we assume
// that order is preserved in the decoding step though this is not
// necessarily part of the contract of that function.
#[test]
fn test_decode_log_body() {
    fn inner(msgs: Vec<LogMsg>) -> TestResult {
        let body = Bytes::from(serde_json::to_string(&msgs).unwrap());
        let api_key = None;

        let decoder = codecs::Decoder::new(
            Box::new(BytesDecoder::new()),
            Box::new(BytesDeserializer::new()),
        );
        let source = DatadogAgentSource::new(true, decoder, "http");
        let events = source.decode_log_body(body, api_key).unwrap();
        assert_eq!(events.len(), msgs.len());
        for (msg, event) in msgs.into_iter().zip(events.into_iter()) {
            let log = event.as_log();
            assert_eq!(log["message"], msg.message.into());
            assert_eq!(log["status"], msg.status.into());
            assert_eq!(log["timestamp"], msg.timestamp.into());
            assert_eq!(log["hostname"], msg.hostname.into());
            assert_eq!(log["service"], msg.service.into());
            assert_eq!(log["ddsource"], msg.ddsource.into());
            assert_eq!(log["ddtags"], msg.ddtags.into());
        }

        TestResult::passed()
    }

    QuickCheck::new().quickcheck(inner as fn(Vec<LogMsg>) -> TestResult);
}

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<DatadogAgentConfig>();
}

async fn source(
    status: EventStatus,
    acknowledgements: bool,
    store_api_key: bool,
    multiple_outputs: bool,
) -> (
    impl Stream<Item = Event>,
    Option<impl Stream<Item = Event>>,
    Option<impl Stream<Item = Event>>,
    SocketAddr,
) {
    let (mut sender, recv) = SourceSender::new_test_finalize(status);
    let mut logs_output = None;
    let mut metrics_output = None;
    if multiple_outputs {
        logs_output = Some(sender.add_outputs(status, "logs".to_string()));
        metrics_output = Some(sender.add_outputs(status, "metrics".to_string()));
    }
    let address = next_addr();
    let context = SourceContext::new_test(sender);
    tokio::spawn(async move {
        DatadogAgentConfig {
            address,
            tls: None,
            store_api_key,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            acknowledgements: acknowledgements.into(),
            multiple_outputs,
        }
        .build(context)
        .await
        .unwrap()
        .await
        .unwrap();
    });
    wait_for_tcp(address).await;
    (recv, logs_output, metrics_output, address)
}

async fn send_with_path(address: SocketAddr, body: &str, headers: HeaderMap, path: &str) -> u16 {
    reqwest::Client::new()
        .post(&format!("http://{}{}", address, path))
        .headers(headers)
        .body(body.to_owned())
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[tokio::test]
async fn full_payload_v1() {
    trace_init();
    let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

    let mut events = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    &serde_json::to_string(&[LogMsg {
                        message: Bytes::from("foo"),
                        timestamp: 123,
                        hostname: Bytes::from("festeburg"),
                        status: Bytes::from("notice"),
                        service: Bytes::from("vector"),
                        ddsource: Bytes::from("curl"),
                        ddtags: Bytes::from("one,two,three"),
                    }])
                    .unwrap(),
                    HeaderMap::new(),
                    "/v1/input/"
                )
                .await
            );
        },
        rx,
        1,
    )
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["message"], "foo".into());
        assert_eq!(log["timestamp"], 123.into());
        assert_eq!(log["hostname"], "festeburg".into());
        assert_eq!(log["status"], "notice".into());
        assert_eq!(log["service"], "vector".into());
        assert_eq!(log["ddsource"], "curl".into());
        assert_eq!(log["ddtags"], "one,two,three".into());
        assert!(event.metadata().datadog_api_key().is_none());
        assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
    }
}

#[tokio::test]
async fn full_payload_v2() {
    trace_init();
    let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

    let mut events = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    &serde_json::to_string(&[LogMsg {
                        message: Bytes::from("foo"),
                        timestamp: 123,
                        hostname: Bytes::from("festeburg"),
                        status: Bytes::from("notice"),
                        service: Bytes::from("vector"),
                        ddsource: Bytes::from("curl"),
                        ddtags: Bytes::from("one,two,three"),
                    }])
                    .unwrap(),
                    HeaderMap::new(),
                    "/api/v2/logs"
                )
                .await
            );
        },
        rx,
        1,
    )
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["message"], "foo".into());
        assert_eq!(log["timestamp"], 123.into());
        assert_eq!(log["hostname"], "festeburg".into());
        assert_eq!(log["status"], "notice".into());
        assert_eq!(log["service"], "vector".into());
        assert_eq!(log["ddsource"], "curl".into());
        assert_eq!(log["ddtags"], "one,two,three".into());
        assert!(event.metadata().datadog_api_key().is_none());
        assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
    }
}

#[tokio::test]
async fn no_api_key() {
    trace_init();
    let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

    let mut events = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    &serde_json::to_string(&[LogMsg {
                        message: Bytes::from("foo"),
                        timestamp: 123,
                        hostname: Bytes::from("festeburg"),
                        status: Bytes::from("notice"),
                        service: Bytes::from("vector"),
                        ddsource: Bytes::from("curl"),
                        ddtags: Bytes::from("one,two,three"),
                    }])
                    .unwrap(),
                    HeaderMap::new(),
                    "/v1/input/"
                )
                .await
            );
        },
        rx,
        1,
    )
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["message"], "foo".into());
        assert_eq!(log["timestamp"], 123.into());
        assert_eq!(log["hostname"], "festeburg".into());
        assert_eq!(log["status"], "notice".into());
        assert_eq!(log["service"], "vector".into());
        assert_eq!(log["ddsource"], "curl".into());
        assert_eq!(log["ddtags"], "one,two,three".into());
        assert!(event.metadata().datadog_api_key().is_none());
        assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
    }
}

#[tokio::test]
async fn api_key_in_url() {
    trace_init();
    let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

    let mut events = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    &serde_json::to_string(&[LogMsg {
                        message: Bytes::from("bar"),
                        timestamp: 456,
                        hostname: Bytes::from("festeburg"),
                        status: Bytes::from("notice"),
                        service: Bytes::from("vector"),
                        ddsource: Bytes::from("curl"),
                        ddtags: Bytes::from("one,two,three"),
                    }])
                    .unwrap(),
                    HeaderMap::new(),
                    "/v1/input/12345678abcdefgh12345678abcdefgh"
                )
                .await
            );
        },
        rx,
        1,
    )
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["message"], "bar".into());
        assert_eq!(log["timestamp"], 456.into());
        assert_eq!(log["hostname"], "festeburg".into());
        assert_eq!(log["status"], "notice".into());
        assert_eq!(log["service"], "vector".into());
        assert_eq!(log["ddsource"], "curl".into());
        assert_eq!(log["ddtags"], "one,two,three".into());
        assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
        assert_eq!(
            &event.metadata().datadog_api_key().as_ref().unwrap()[..],
            "12345678abcdefgh12345678abcdefgh"
        );
    }
}

#[tokio::test]
async fn api_key_in_query_params() {
    trace_init();
    let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

    let mut events = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    &serde_json::to_string(&[LogMsg {
                        message: Bytes::from("bar"),
                        timestamp: 456,
                        hostname: Bytes::from("festeburg"),
                        status: Bytes::from("notice"),
                        service: Bytes::from("vector"),
                        ddsource: Bytes::from("curl"),
                        ddtags: Bytes::from("one,two,three"),
                    }])
                    .unwrap(),
                    HeaderMap::new(),
                    "/api/v2/logs?dd-api-key=12345678abcdefgh12345678abcdefgh"
                )
                .await
            );
        },
        rx,
        1,
    )
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["message"], "bar".into());
        assert_eq!(log["timestamp"], 456.into());
        assert_eq!(log["hostname"], "festeburg".into());
        assert_eq!(log["status"], "notice".into());
        assert_eq!(log["service"], "vector".into());
        assert_eq!(log["ddsource"], "curl".into());
        assert_eq!(log["ddtags"], "one,two,three".into());
        assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
        assert_eq!(
            &event.metadata().datadog_api_key().as_ref().unwrap()[..],
            "12345678abcdefgh12345678abcdefgh"
        );
    }
}

#[tokio::test]
async fn api_key_in_header() {
    trace_init();
    let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        "dd-api-key",
        "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
    );

    let mut events = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    &serde_json::to_string(&[LogMsg {
                        message: Bytes::from("baz"),
                        timestamp: 789,
                        hostname: Bytes::from("festeburg"),
                        status: Bytes::from("notice"),
                        service: Bytes::from("vector"),
                        ddsource: Bytes::from("curl"),
                        ddtags: Bytes::from("one,two,three"),
                    }])
                    .unwrap(),
                    headers,
                    "/v1/input/"
                )
                .await
            );
        },
        rx,
        1,
    )
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["message"], "baz".into());
        assert_eq!(log["timestamp"], 789.into());
        assert_eq!(log["hostname"], "festeburg".into());
        assert_eq!(log["status"], "notice".into());
        assert_eq!(log["service"], "vector".into());
        assert_eq!(log["ddsource"], "curl".into());
        assert_eq!(log["ddtags"], "one,two,three".into());
        assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
        assert_eq!(
            &event.metadata().datadog_api_key().as_ref().unwrap()[..],
            "12345678abcdefgh12345678abcdefgh"
        );
    }
}

#[tokio::test]
async fn delivery_failure() {
    trace_init();
    let (rx, _, _, addr) = source(EventStatus::Rejected, true, true, false).await;

    spawn_collect_n(
        async move {
            assert_eq!(
                400,
                send_with_path(
                    addr,
                    &serde_json::to_string(&[LogMsg {
                        message: Bytes::from("foo"),
                        timestamp: 123,
                        hostname: Bytes::from("festeburg"),
                        status: Bytes::from("notice"),
                        service: Bytes::from("vector"),
                        ddsource: Bytes::from("curl"),
                        ddtags: Bytes::from("one,two,three"),
                    }])
                    .unwrap(),
                    HeaderMap::new(),
                    "/v1/input/"
                )
                .await
            );
        },
        rx,
        1,
    )
    .await;
}

#[tokio::test]
async fn ignores_disabled_acknowledgements() {
    trace_init();
    let (rx, _, _, addr) = source(EventStatus::Rejected, false, true, false).await;

    let events = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    &serde_json::to_string(&[LogMsg {
                        message: Bytes::from("foo"),
                        timestamp: 123,
                        hostname: Bytes::from("festeburg"),
                        status: Bytes::from("notice"),
                        service: Bytes::from("vector"),
                        ddsource: Bytes::from("curl"),
                        ddtags: Bytes::from("one,two,three"),
                    }])
                    .unwrap(),
                    HeaderMap::new(),
                    "/v1/input/"
                )
                .await
            );
        },
        rx,
        1,
    )
    .await;

    assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn ignores_api_key() {
    trace_init();
    let (rx, _, _, addr) = source(EventStatus::Delivered, true, false, false).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        "dd-api-key",
        "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
    );

    let mut events = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    &serde_json::to_string(&[LogMsg {
                        message: Bytes::from("baz"),
                        timestamp: 789,
                        hostname: Bytes::from("festeburg"),
                        status: Bytes::from("notice"),
                        service: Bytes::from("vector"),
                        ddsource: Bytes::from("curl"),
                        ddtags: Bytes::from("one,two,three"),
                    }])
                    .unwrap(),
                    headers,
                    "/v1/input/12345678abcdefgh12345678abcdefgh"
                )
                .await
            );
        },
        rx,
        1,
    )
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["message"], "baz".into());
        assert_eq!(log["timestamp"], 789.into());
        assert_eq!(log["hostname"], "festeburg".into());
        assert_eq!(log["status"], "notice".into());
        assert_eq!(log["service"], "vector".into());
        assert_eq!(log["ddsource"], "curl".into());
        assert_eq!(log["ddtags"], "one,two,three".into());
        assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
        assert!(event.metadata().datadog_api_key().is_none());
    }
}

#[tokio::test]
async fn decode_series_endpoints() {
    trace_init();
    let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        "dd-api-key",
        "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
    );

    let dd_metric_request = DatadogSeriesRequest {
        series: vec![
            DatadogSeriesMetric {
                metric: "dd_gauge".to_string(),
                r#type: DatadogMetricType::Gauge,
                interval: None,
                points: vec![
                    DatadogPoint(1542182950, 3.14),
                    DatadogPoint(1542182951, 3.1415),
                ],
                tags: Some(vec!["foo:bar".to_string()]),
                host: Some("random_host".to_string()),
                source_type_name: None,
                device: None,
            },
            DatadogSeriesMetric {
                metric: "dd_rate".to_string(),
                r#type: DatadogMetricType::Rate,
                interval: Some(10),
                points: vec![DatadogPoint(1542182950, 3.14)],
                tags: Some(vec!["foo:bar:baz".to_string()]),
                host: Some("another_random_host".to_string()),
                source_type_name: None,
                device: None,
            },
            DatadogSeriesMetric {
                metric: "dd_count".to_string(),
                r#type: DatadogMetricType::Count,
                interval: None,
                points: vec![DatadogPoint(1542182955, 16777216_f64)],
                tags: Some(vec!["foobar".to_string()]),
                host: Some("a_host".to_string()),
                source_type_name: None,
                device: None,
            },
        ],
    };
    let events = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    &serde_json::to_string(&dd_metric_request).unwrap(),
                    headers,
                    "/api/v1/series"
                )
                .await
            );
        },
        rx,
        4,
    )
    .await;

    {
        let mut metric = events[0].as_metric();
        assert_eq!(metric.name(), "dd_gauge");
        assert_eq!(
            metric.timestamp(),
            Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 10))
        );
        assert_eq!(metric.kind(), MetricKind::Absolute);
        assert_eq!(*metric.value(), MetricValue::Gauge { value: 3.14 });
        assert_eq!(metric.tags().unwrap()["host"], "random_host".to_string());
        assert_eq!(metric.tags().unwrap()["foo"], "bar".to_string());

        assert_eq!(
            &events[0].metadata().datadog_api_key().as_ref().unwrap()[..],
            "12345678abcdefgh12345678abcdefgh"
        );

        metric = events[1].as_metric();
        assert_eq!(metric.name(), "dd_gauge");
        assert_eq!(
            metric.timestamp(),
            Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 11))
        );
        assert_eq!(metric.kind(), MetricKind::Absolute);
        assert_eq!(*metric.value(), MetricValue::Gauge { value: 3.1415 });
        assert_eq!(metric.tags().unwrap()["host"], "random_host".to_string());
        assert_eq!(metric.tags().unwrap()["foo"], "bar".to_string());

        assert_eq!(
            &events[1].metadata().datadog_api_key().as_ref().unwrap()[..],
            "12345678abcdefgh12345678abcdefgh"
        );

        metric = events[2].as_metric();
        assert_eq!(metric.name(), "dd_rate");
        assert_eq!(
            metric.timestamp(),
            Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 10))
        );
        assert_eq!(metric.kind(), MetricKind::Incremental);
        assert_eq!(
            *metric.value(),
            MetricValue::Counter {
                value: 3.14 * (10_f64)
            }
        );
        assert_eq!(
            metric.tags().unwrap()["host"],
            "another_random_host".to_string()
        );
        assert_eq!(metric.tags().unwrap()["foo"], "bar:baz".to_string());

        assert_eq!(
            &events[2].metadata().datadog_api_key().as_ref().unwrap()[..],
            "12345678abcdefgh12345678abcdefgh"
        );

        metric = events[3].as_metric();
        assert_eq!(metric.name(), "dd_count");
        assert_eq!(
            metric.timestamp(),
            Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 15))
        );
        assert_eq!(metric.kind(), MetricKind::Incremental);
        assert_eq!(
            *metric.value(),
            MetricValue::Counter {
                value: 16777216_f64
            }
        );
        assert_eq!(metric.tags().unwrap()["host"], "a_host".to_string());
        assert_eq!(metric.tags().unwrap()["foobar"], "".to_string());

        assert_eq!(
            &events[3].metadata().datadog_api_key().as_ref().unwrap()[..],
            "12345678abcdefgh12345678abcdefgh"
        );
    }
}

#[tokio::test]
async fn decode_sketches() {
    trace_init();
    let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        "dd-api-key",
        "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
    );

    let mut buf = Vec::new();
    let sketch = dd_proto::sketch_payload::Sketch {
        metric: "dd_sketch".to_string(),
        tags: vec!["foo:bar".to_string(), "foobar".to_string()],
        host: "a_host".to_string(),
        distributions: Vec::new(),
        dogsketches: vec![dd_proto::sketch_payload::sketch::Dogsketch {
            ts: 1542182950,
            cnt: 2,
            min: 16.0,
            max: 31.0,
            avg: 23.5,
            sum: 74.0,
            k: vec![1517, 1559],
            n: vec![1, 1],
        }],
    };

    let sketch_payload = dd_proto::SketchPayload {
        metadata: None,
        sketches: vec![sketch],
    };

    sketch_payload.encode(&mut buf).unwrap();

    let events = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    unsafe { str::from_utf8_unchecked(&buf) },
                    headers,
                    "/api/beta/sketches"
                )
                .await
            );
        },
        rx,
        1,
    )
    .await;

    {
        let metric = events[0].as_metric();
        assert_eq!(metric.name(), "dd_sketch");
        assert_eq!(
            metric.timestamp(),
            Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 10))
        );
        assert_eq!(metric.kind(), MetricKind::Incremental);
        assert_eq!(metric.tags().unwrap()["host"], "a_host".to_string());
        assert_eq!(metric.tags().unwrap()["foo"], "bar".to_string());
        assert_eq!(metric.tags().unwrap()["foobar"], "".to_string());

        let s = &*metric.value();
        assert!(matches!(s, MetricValue::Sketch { .. }));
        if let MetricValue::Sketch {
            sketch: MetricSketch::AgentDDSketch(ddsketch),
        } = s
        {
            assert_eq!(ddsketch.bins().len(), 2);
            assert_eq!(ddsketch.count(), 2);
            assert_eq!(ddsketch.min(), Some(16.0));
            assert_eq!(ddsketch.max(), Some(31.0));
            assert_eq!(ddsketch.sum(), Some(74.0));
            assert_eq!(ddsketch.avg(), Some(23.5));
        }

        assert_eq!(
            &events[0].metadata().datadog_api_key().as_ref().unwrap()[..],
            "12345678abcdefgh12345678abcdefgh"
        );
    }
}

#[tokio::test]
async fn split_outputs() {
    init_test();
    let (_, rx_logs, rx_metrics, addr) = source(EventStatus::Delivered, true, true, true).await;

    let mut headers_for_log = HeaderMap::new();
    headers_for_log.insert(
        "dd-api-key",
        "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
    );

    let mut log_event = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    &serde_json::to_string(&[LogMsg {
                        message: Bytes::from("baz"),
                        timestamp: 789,
                        hostname: Bytes::from("festeburg"),
                        status: Bytes::from("notice"),
                        service: Bytes::from("vector"),
                        ddsource: Bytes::from("curl"),
                        ddtags: Bytes::from("one,two,three"),
                    }])
                    .unwrap(),
                    headers_for_log,
                    "/v1/input/"
                )
                .await
            );
        },
        rx_logs.unwrap(),
        1,
    )
    .await;

    let mut headers_for_metric = HeaderMap::new();
    headers_for_metric.insert(
        "dd-api-key",
        "abcdefgh12345678abcdefgh12345678".parse().unwrap(),
    );
    let dd_metric_request = DatadogSeriesRequest {
        series: vec![DatadogSeriesMetric {
            metric: "dd_gauge".to_string(),
            r#type: DatadogMetricType::Gauge,
            interval: None,
            points: vec![
                DatadogPoint(1542182950, 3.14),
                DatadogPoint(1542182951, 3.1415),
            ],
            tags: Some(vec!["foo:bar".to_string()]),
            host: Some("random_host".to_string()),
            source_type_name: None,
            device: None,
        }],
    };
    let mut metric_event = spawn_collect_n(
        async move {
            assert_eq!(
                200,
                send_with_path(
                    addr,
                    &serde_json::to_string(&dd_metric_request).unwrap(),
                    headers_for_metric,
                    "/api/v1/series"
                )
                .await
            );
        },
        rx_metrics.unwrap(),
        1,
    )
    .await;

    {
        let event = metric_event.remove(0);
        let metric = event.as_metric();
        assert_eq!(metric.name(), "dd_gauge");
        assert_eq!(
            metric.timestamp(),
            Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 10))
        );
        assert_eq!(metric.kind(), MetricKind::Absolute);
        assert_eq!(*metric.value(), MetricValue::Gauge { value: 3.14 });
        assert_eq!(metric.tags().unwrap()["host"], "random_host".to_string());
        assert_eq!(metric.tags().unwrap()["foo"], "bar".to_string());

        assert_eq!(
            &event.metadata().datadog_api_key().as_ref().unwrap()[..],
            "abcdefgh12345678abcdefgh12345678"
        );
    }

    {
        let event = log_event.remove(0);
        let log = event.as_log();
        assert_eq!(log["message"], "baz".into());
        assert_eq!(log["timestamp"], 789.into());
        assert_eq!(log["hostname"], "festeburg".into());
        assert_eq!(log["status"], "notice".into());
        assert_eq!(log["service"], "vector".into());
        assert_eq!(log["ddsource"], "curl".into());
        assert_eq!(log["ddtags"], "one,two,three".into());
        assert_eq!(log[log_schema().source_type_key()], "datadog_agent".into());
        assert_eq!(
            &event.metadata().datadog_api_key().as_ref().unwrap()[..],
            "12345678abcdefgh12345678abcdefgh"
        );
    }

    COMPONENT_MULTIPLE_OUTPUTS_TESTS.assert(&["output"]);
}
