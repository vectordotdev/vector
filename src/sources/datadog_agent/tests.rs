use std::{
    collections::{BTreeMap, HashMap},
    iter::FromIterator,
    net::SocketAddr,
    str,
};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{Stream, StreamExt};
use http::HeaderMap;
use indoc::indoc;
use ordered_float::NotNan;
use prost::Message;
use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
use similar_asserts::assert_eq;
use vector_lib::lookup::{owned_value_path, OwnedTargetPath};
use vector_lib::{
    codecs::{
        decoding::{Deserializer, DeserializerConfig, Framer},
        BytesDecoder, BytesDeserializer,
    },
    config::DataType,
};
use vector_lib::{
    config::LogNamespace,
    event::{metric::TagValue, MetricTags},
    metric_tags,
};
use vrl::compiler::value::Collection;
use vrl::value::{Kind, ObjectMap};

use crate::schema::Definition;
use crate::{
    common::datadog::{DatadogMetricType, DatadogPoint, DatadogSeriesMetric},
    config::{SourceConfig, SourceContext},
    event::{
        into_event_stream,
        metric::{MetricKind, MetricSketch, MetricValue},
        Event, EventStatus, Metric, Value,
    },
    schema,
    serde::{default_decoding, default_framing_message_based},
    sources::datadog_agent::{
        ddmetric_proto, ddtrace_proto, logs::decode_log_body, metrics::DatadogSeriesRequest,
        DatadogAgentConfig, DatadogAgentSource, LogMsg, LOGS, METRICS, TRACES,
    },
    test_util::{
        components::{assert_source_compliance, HTTP_PUSH_SOURCE_TAGS},
        next_addr, spawn_collect_n, trace_init, wait_for_tcp,
    },
    SourceSender,
};

fn test_logs_schema_definition() -> schema::Definition {
    schema::Definition::empty_legacy_namespace().with_event_field(
        &owned_value_path!("a log field"),
        Kind::integer().or_bytes(),
        Some("log field"),
    )
}

impl Arbitrary for LogMsg {
    fn arbitrary(g: &mut Gen) -> Self {
        LogMsg {
            message: Bytes::from(String::arbitrary(g)),
            status: Bytes::from(String::arbitrary(g)),
            timestamp: Utc
                .timestamp_millis_opt(u32::arbitrary(g) as i64)
                .single()
                .expect("invalid timestamp"),
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
        let decoder = crate::codecs::Decoder::new(
            Framer::Bytes(BytesDecoder::new()),
            Deserializer::Bytes(BytesDeserializer),
        );

        let source = DatadogAgentSource::new(
            true,
            decoder,
            "http",
            Some(test_logs_schema_definition()),
            LogNamespace::Legacy,
        );

        let events = decode_log_body(body, api_key, &source).unwrap();
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

            assert_eq!(
                event.metadata().schema_definition(),
                &test_logs_schema_definition()
            );
        }

        TestResult::passed()
    }

    QuickCheck::new().quickcheck(inner as fn(Vec<LogMsg>) -> TestResult);
}

#[test]
fn test_decode_log_body_empty_object() {
    let body = Bytes::from("{}");
    let api_key = None;
    let decoder = crate::codecs::Decoder::new(
        Framer::Bytes(BytesDecoder::new()),
        Deserializer::Bytes(BytesDeserializer),
    );

    let source = DatadogAgentSource::new(
        true,
        decoder,
        "http",
        Some(test_logs_schema_definition()),
        LogNamespace::Legacy,
    );

    let events = decode_log_body(body, api_key, &source).unwrap();
    assert_eq!(events.len(), 0);
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
    impl Stream<Item = Event> + Unpin,
    Option<impl Stream<Item = Event>>,
    Option<impl Stream<Item = Event>>,
    SocketAddr,
) {
    let (mut sender, recv) = SourceSender::new_test_finalize(status);
    let mut logs_output = None;
    let mut metrics_output = None;
    if multiple_outputs {
        logs_output = Some(
            sender
                .add_outputs(status, "logs".to_string())
                .flat_map(into_event_stream),
        );
        metrics_output = Some(
            sender
                .add_outputs(status, "metrics".to_string())
                .flat_map(into_event_stream),
        );
    }
    let address = next_addr();
    let config = toml::from_str::<DatadogAgentConfig>(&format!(
        indoc! { r#"
            address = "{}"
            compression = "none"
            store_api_key = {}
            acknowledgements = {}
            multiple_outputs = {}
            trace_proto = "v1v2"
        "#},
        address, store_api_key, acknowledgements, multiple_outputs
    ))
    .unwrap();
    let schema_definitions =
        HashMap::from([(Some(LOGS.to_owned()), test_logs_schema_definition())]);
    let context = SourceContext::new_test(sender, Some(schema_definitions));
    tokio::spawn(async move {
        config.build(context).await.unwrap().await.unwrap();
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
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("foo"),
                            timestamp: Utc
                                .timestamp_opt(123, 0)
                                .single()
                                .expect("invalid timestamp"),
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
            assert_eq!(
                log["timestamp"],
                Utc.timestamp_opt(123, 0)
                    .single()
                    .expect("invalid timestamp")
                    .into()
            );
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert!(event.metadata().datadog_api_key().is_none());
            assert_eq!(*log.get_source_type().unwrap(), "datadog_agent".into());
            assert_eq!(
                event.metadata().schema_definition(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn full_payload_v2() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("foo"),
                            timestamp: Utc
                                .timestamp_opt(123, 0)
                                .single()
                                .expect("invalid timestamp"),
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
            assert_eq!(
                log["timestamp"],
                Utc.timestamp_opt(123, 0)
                    .single()
                    .expect("invalid timestamp")
                    .into()
            );
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert!(event.metadata().datadog_api_key().is_none());
            assert_eq!(*log.get_source_type().unwrap(), "datadog_agent".into());
            assert_eq!(
                event.metadata().schema_definition(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn no_api_key() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("foo"),
                            timestamp: Utc
                                .timestamp_opt(123, 0)
                                .single()
                                .expect("invalid timestamp"),
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
            assert_eq!(
                log["timestamp"],
                Utc.timestamp_opt(123, 0)
                    .single()
                    .expect("invalid timestamp")
                    .into()
            );
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert!(event.metadata().datadog_api_key().is_none());
            assert_eq!(*log.get_source_type().unwrap(), "datadog_agent".into());
            assert_eq!(
                event.metadata().schema_definition(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn api_key_in_url() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("bar"),
                            timestamp: Utc
                                .timestamp_opt(456, 0)
                                .single()
                                .expect("invalid timestamp"),
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
            assert_eq!(
                log["timestamp"],
                Utc.timestamp_opt(456, 0)
                    .single()
                    .expect("invalid timestamp")
                    .into()
            );
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert_eq!(*log.get_source_type().unwrap(), "datadog_agent".into());
            assert_eq!(
                &event.metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
            assert_eq!(
                event.metadata().schema_definition(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn api_key_in_query_params() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("bar"),
                            timestamp: Utc
                                .timestamp_opt(456, 0)
                                .single()
                                .expect("invalid timestamp"),
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
            assert_eq!(
                log["timestamp"],
                Utc.timestamp_opt(456, 0)
                    .single()
                    .expect("invalid timestamp")
                    .into()
            );
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert_eq!(*log.get_source_type().unwrap(), "datadog_agent".into());
            assert_eq!(
                &event.metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
            assert_eq!(
                event.metadata().schema_definition(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn api_key_in_header() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
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
                            timestamp: Utc
                                .timestamp_opt(789, 0)
                                .single()
                                .expect("invalid timestamp"),
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
            assert_eq!(
                log["timestamp"],
                Utc.timestamp_opt(789, 0)
                    .single()
                    .expect("invalid timestamp")
                    .into()
            );
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert_eq!(*log.get_source_type().unwrap(), "datadog_agent".into());
            assert_eq!(
                &event.metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
            assert_eq!(
                event.metadata().schema_definition(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
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
                        timestamp: Utc
                            .timestamp_opt(123, 0)
                            .single()
                            .expect("invalid timestamp"),
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
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr) = source(EventStatus::Rejected, false, true, false).await;

        let events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        &serde_json::to_string(&[LogMsg {
                            message: Bytes::from("foo"),
                            timestamp: Utc
                                .timestamp_opt(123, 0)
                                .single()
                                .expect("invalid timestamp"),
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
    })
    .await;
}

#[tokio::test]
async fn ignores_api_key() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
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
                            timestamp: Utc
                                .timestamp_opt(789, 0)
                                .single()
                                .expect("invalid timestamp"),
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
            assert_eq!(
                log["timestamp"],
                Utc.timestamp_opt(789, 0)
                    .single()
                    .expect("invalid timestamp")
                    .into()
            );
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert_eq!(*log.get_source_type().unwrap(), "datadog_agent".into());
            assert!(event.metadata().datadog_api_key().is_none());
            assert_eq!(
                event.metadata().schema_definition(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn decode_series_endpoint_v1() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
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
                    metadata: None,
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
                    metadata: None,
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
                    metadata: None,
                },
                DatadogSeriesMetric {
                    metric: "system.disk.free".to_string(),
                    r#type: DatadogMetricType::Count,
                    interval: None,
                    points: vec![DatadogPoint(1542182955, 16777216_f64)],
                    tags: None,
                    host: None,
                    source_type_name: None,
                    device: None,
                    metadata: None,
                },
                DatadogSeriesMetric {
                    metric: "system.disk".to_string(),
                    r#type: DatadogMetricType::Count,
                    interval: None,
                    points: vec![DatadogPoint(1542182955, 16777216_f64)],
                    tags: None,
                    host: None,
                    source_type_name: None,
                    device: None,
                    metadata: None,
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
            6,
        )
        .await;

        {
            let mut metric = events[0].as_metric();
            assert_eq!(metric.name(), "dd_gauge");
            assert_eq!(metric.namespace(), None);
            assert_eq!(
                metric.timestamp(),
                Some(
                    Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                        .single()
                        .expect("invalid timestamp")
                )
            );
            assert_eq!(metric.kind(), MetricKind::Absolute);
            assert_eq!(*metric.value(), MetricValue::Gauge { value: 3.14 });
            assert_tags(
                metric,
                metric_tags!(
                    "host" => "random_host",
                    "foo" => "bar",
                ),
            );

            assert_eq!(
                &events[0].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            metric = events[1].as_metric();
            assert_eq!(metric.name(), "dd_gauge");
            assert_eq!(metric.namespace(), None);
            assert_eq!(
                metric.timestamp(),
                Some(
                    Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 11)
                        .single()
                        .expect("invalid timestamp")
                )
            );
            assert_eq!(metric.kind(), MetricKind::Absolute);
            assert_eq!(*metric.value(), MetricValue::Gauge { value: 3.1415 });
            assert_tags(
                metric,
                metric_tags!(
                    "host" => "random_host",
                    "foo" => "bar",
                ),
            );

            assert_eq!(
                &events[1].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            metric = events[2].as_metric();
            assert_eq!(metric.name(), "dd_rate");
            assert_eq!(metric.namespace(), None);
            assert_eq!(
                metric.timestamp(),
                Some(
                    Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                        .single()
                        .expect("invalid timestamp")
                )
            );
            assert_eq!(metric.kind(), MetricKind::Incremental);
            assert_eq!(
                *metric.value(),
                MetricValue::Counter {
                    value: 3.14 * (10_f64)
                }
            );
            assert_tags(
                metric,
                metric_tags!(
                    "host" => "another_random_host",
                    "foo" => "bar:baz",
                ),
            );

            assert_eq!(
                &events[2].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            metric = events[3].as_metric();
            assert_eq!(metric.name(), "dd_count");
            assert_eq!(
                metric.timestamp(),
                Some(
                    Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 15)
                        .single()
                        .expect("invalid timestamp")
                )
            );
            assert_eq!(metric.kind(), MetricKind::Incremental);
            assert_eq!(
                *metric.value(),
                MetricValue::Counter {
                    value: 16777216_f64
                }
            );
            assert_tags(
                metric,
                metric_tags!(
                    "host" => "a_host",
                    "foobar" => TagValue::Bare,
                ),
            );

            metric = events[4].as_metric();
            assert_eq!(metric.name(), "disk.free");
            assert_eq!(metric.namespace(), Some("system"));

            metric = events[5].as_metric();
            assert_eq!(metric.name(), "disk");
            assert_eq!(metric.namespace(), Some("system"));

            assert_eq!(
                &events[3].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn decode_sketches() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

        let mut headers = HeaderMap::new();
        headers.insert(
            "dd-api-key",
            "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
        );

        let mut buf = Vec::new();
        let sketch = ddmetric_proto::sketch_payload::Sketch {
            metric: "dd_sketch".to_string(),
            tags: vec![
                "foo:bar".to_string(),
                "foo:baz".to_string(),
                "foobar".to_string(),
            ],
            host: "a_host".to_string(),
            distributions: Vec::new(),
            dogsketches: vec![ddmetric_proto::sketch_payload::sketch::Dogsketch {
                ts: 1542182950,
                cnt: 2,
                min: 16.0,
                max: 31.0,
                avg: 23.5,
                sum: 74.0,
                k: vec![1517, 1559],
                n: vec![1, 1],
            }],
            metadata: Some(ddmetric_proto::Metadata {
                origin: Some(ddmetric_proto::Origin {
                    origin_product: 10,
                    origin_category: 11,
                    origin_service: 9,
                }),
            }),
        };

        let sketch_payload = ddmetric_proto::SketchPayload {
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
                Some(
                    Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                        .single()
                        .expect("invalid timestamp")
                )
            );
            assert_eq!(metric.kind(), MetricKind::Incremental);
            assert_tags(
                metric,
                metric_tags!(
                    "host" => "a_host",
                    "foo" => "bar",
                    "foo" => "baz",
                    "foobar" => TagValue::Bare,
                ),
            );
            let s = metric.value();
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

            let event_origin = &events[0].metadata().datadog_origin_metadata().unwrap();
            assert_eq!(event_origin.product().unwrap(), 10);
            assert_eq!(event_origin.category().unwrap(), 11);
            assert_eq!(event_origin.service().unwrap(), 9);
        }
    })
    .await;
}

#[tokio::test]
async fn decode_traces() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

        let mut headers = HeaderMap::new();
        headers.insert(
            "dd-api-key",
            "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
        );
        headers.insert("X-Datadog-Reported-Languages", "ada".parse().unwrap());

        let mut buf_v1 = Vec::new();

        let span = ddtrace_proto::Span {
            service: "a_service".to_string(),
            name: "a_name".to_string(),
            resource: "a_resource".to_string(),
            trace_id: 123u64,
            span_id: 456u64,
            parent_id: 789u64,
            start: 1_431_648_000_000_001i64,
            duration: 1_000_000_000i64,
            error: 404i32,
            meta: BTreeMap::from_iter([("foo".to_string(), "bar".to_string())].into_iter()),
            metrics: BTreeMap::from_iter([("a_metrics".to_string(), 0.577f64)].into_iter()),
            r#type: "a_type".to_string(),
            meta_struct: BTreeMap::new(),
        };

        let trace = ddtrace_proto::ApiTrace {
            trace_id: 123u64,
            spans: vec![span.clone()],
            start_time: 1_431_648_000_000_001i64,
            end_time: 1_431_649_000_000_001i64,
        };

        let payload_v1 = ddtrace_proto::TracePayload {
            host_name: "a_hostname".to_string(),
            env: "an_environment".to_string(),
            traces: vec![trace],
            transactions: vec![span.clone()],
            // Other filea
            tracer_payloads: vec![],
            tags: BTreeMap::new(),
            agent_version: "".to_string(),
            target_tps: 0f64,
            error_tps: 0f64,
        };

        payload_v1.encode(&mut buf_v1).unwrap();

        let mut buf_v2 = Vec::new();

        let chunk = ddtrace_proto::TraceChunk {
            priority: 42i32,
            origin: "an_origin".to_string(),
            dropped_trace: false,
            spans: vec![span],
            tags: BTreeMap::from_iter([("a".to_string(), "tag".to_string())].into_iter()),
        };

        let tracer_payload = ddtrace_proto::TracerPayload {
            container_id: "an_id".to_string(),
            language_name: "plop".to_string(),
            language_version: "v33".to_string(),
            tracer_version: "v577".to_string(),
            runtime_id: "123abc".to_string(),
            chunks: vec![chunk],
            env: "env".to_string(),
            tags: BTreeMap::from_iter([("another".to_string(), "tag".to_string())].into_iter()),
            hostname: "hostname".to_string(),
            app_version: "v314".to_string(),
        };

        let payload_v2 = ddtrace_proto::TracePayload {
            host_name: "a_hostname".to_string(),
            env: "env".to_string(),
            traces: vec![],
            transactions: vec![],
            tracer_payloads: vec![tracer_payload],
            tags: BTreeMap::new(),
            agent_version: "v1.23456".to_string(),
            target_tps: 10f64,
            error_tps: 10f64,
        };

        payload_v2.encode(&mut buf_v2).unwrap();

        let events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        unsafe { str::from_utf8_unchecked(&buf_v1) },
                        headers.clone(),
                        "/api/v0.2/traces"
                    )
                    .await
                );
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        unsafe { str::from_utf8_unchecked(&buf_v2) },
                        headers,
                        "/api/v0.2/traces"
                    )
                    .await
                );
            },
            rx,
            3,
        )
        .await;

        {
            let trace_v1 = events[0].as_trace();
            assert_eq!(trace_v1.as_map()["host"], "a_hostname".into());
            assert_eq!(trace_v1.as_map()["env"], "an_environment".into());
            assert_eq!(trace_v1.as_map()["language_name"], "ada".into());
            assert!(trace_v1.contains("spans"));
            assert_eq!(trace_v1.as_map()["spans"].as_array().unwrap().len(), 1);
            let span_from_trace_v1 = trace_v1.as_map()["spans"].as_array().unwrap()[0]
                .as_object()
                .unwrap();
            assert_eq!(span_from_trace_v1["service"], "a_service".into());
            assert_eq!(span_from_trace_v1["name"], "a_name".into());
            assert_eq!(span_from_trace_v1["resource"], "a_resource".into());
            assert_eq!(span_from_trace_v1["trace_id"], Value::Integer(123));
            assert_eq!(span_from_trace_v1["span_id"], Value::Integer(456));
            assert_eq!(span_from_trace_v1["parent_id"], Value::Integer(789));
            assert_eq!(
                span_from_trace_v1["start"],
                Value::from(Utc.timestamp_nanos(1_431_648_000_000_001i64))
            );
            assert_eq!(
                span_from_trace_v1["duration"],
                Value::Integer(1_000_000_000)
            );
            assert_eq!(span_from_trace_v1["error"], Value::Integer(404));
            assert_eq!(span_from_trace_v1["meta"].as_object().unwrap().len(), 1);
            assert_eq!(
                span_from_trace_v1["meta"].as_object().unwrap()["foo"],
                "bar".into()
            );
            assert_eq!(span_from_trace_v1["metrics"].as_object().unwrap().len(), 1);
            assert_eq!(
                span_from_trace_v1["metrics"].as_object().unwrap()["a_metrics"],
                0.577.into()
            );
            assert_eq!(
                &events[0].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            let apm_event = events[1].as_trace();
            assert!(apm_event.contains("spans"));
            assert_eq!(apm_event.as_map()["host"], "a_hostname".into());
            assert_eq!(apm_event.as_map()["env"], "an_environment".into());
            assert_eq!(apm_event.as_map()["language_name"], "ada".into());
            let span_from_apm_event = apm_event.as_map()["spans"].as_array().unwrap()[0]
                .as_object()
                .unwrap();

            assert_eq!(span_from_apm_event["service"], "a_service".into());
            assert_eq!(span_from_apm_event["name"], "a_name".into());
            assert_eq!(span_from_apm_event["resource"], "a_resource".into());

            assert_eq!(
                &events[1].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            let trace_v2 = events[2].as_trace();
            assert_eq!(trace_v2.as_map()["host"], "a_hostname".into());
            assert_eq!(trace_v2.as_map()["env"], "env".into());

            assert_eq!(
                trace_v2.as_map()["tags"],
                Value::Object(ObjectMap::from_iter(
                    [("a".into(), "tag".into()), ("another".into(), "tag".into())].into_iter()
                ))
            );

            assert_eq!(trace_v2.as_map()["language_name"], "plop".into());
            assert_eq!(trace_v2.as_map()["language_version"], "v33".into());
            assert_eq!(trace_v2.as_map()["container_id"], "an_id".into());
            assert_eq!(trace_v2.as_map()["origin"], "an_origin".into());
            assert_eq!(trace_v2.as_map()["tracer_version"], "v577".into());
            assert_eq!(trace_v2.as_map()["runtime_id"], "123abc".into());
            assert_eq!(trace_v2.as_map()["app_version"], "v314".into());
            assert_eq!(trace_v2.as_map()["priority"], Value::Integer(42));
            assert_eq!(
                trace_v2.as_map()["target_tps"],
                Value::Float(NotNan::new(10.0f64).unwrap())
            );
            assert_eq!(
                trace_v2.as_map()["error_tps"],
                Value::Float(NotNan::new(10.0f64).unwrap())
            );
            assert!(trace_v2.contains("spans"));
            assert_eq!(trace_v2.as_map()["spans"].as_array().unwrap().len(), 1);
            let span_from_trace_v2 = trace_v2.as_map()["spans"].as_array().unwrap()[0]
                .as_object()
                .unwrap();
            assert_eq!(span_from_trace_v2["service"], "a_service".into());
            assert_eq!(span_from_trace_v2["name"], "a_name".into());
            assert_eq!(span_from_trace_v2["resource"], "a_resource".into());
            assert_eq!(span_from_trace_v2["trace_id"], Value::Integer(123));
            assert_eq!(span_from_trace_v2["span_id"], Value::Integer(456));
            assert_eq!(span_from_trace_v2["parent_id"], Value::Integer(789));
            assert_eq!(
                span_from_trace_v2["start"],
                Value::from(Utc.timestamp_nanos(1_431_648_000_000_001i64))
            );
            assert_eq!(
                span_from_trace_v2["duration"],
                Value::Integer(1_000_000_000)
            );
            assert_eq!(span_from_trace_v2["error"], Value::Integer(404));
            assert_eq!(span_from_trace_v2["meta"].as_object().unwrap().len(), 1);
            assert_eq!(
                span_from_trace_v2["meta"].as_object().unwrap()["foo"],
                "bar".into()
            );
            assert_eq!(span_from_trace_v2["metrics"].as_object().unwrap().len(), 1);
            assert_eq!(
                span_from_trace_v2["metrics"].as_object().unwrap()["a_metrics"],
                0.577.into()
            );
            assert_eq!(
                &events[2].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn split_outputs() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
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
                            timestamp: Utc
                                .timestamp_opt(789, 0)
                                .single()
                                .expect("invalid timestamp"),
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
                metadata: None,
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
                Some(
                    Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                        .single()
                        .expect("invalid timestamp")
                )
            );
            assert_eq!(metric.kind(), MetricKind::Absolute);
            assert_eq!(*metric.value(), MetricValue::Gauge { value: 3.14 });
            assert_tags(
                metric,
                metric_tags!(
                    "host" => "random_host",
                    "foo" => "bar",
                ),
            );
            assert_eq!(
                &event.metadata().datadog_api_key().as_ref().unwrap()[..],
                "abcdefgh12345678abcdefgh12345678"
            );
        }

        {
            let event = log_event.remove(0);
            let log = event.as_log();
            assert_eq!(log["message"], "baz".into());
            assert_eq!(
                log["timestamp"],
                Utc.timestamp_opt(789, 0)
                    .single()
                    .expect("invalid timestamp")
                    .into()
            );
            assert_eq!(log["hostname"], "festeburg".into());
            assert_eq!(log["status"], "notice".into());
            assert_eq!(log["service"], "vector".into());
            assert_eq!(log["ddsource"], "curl".into());
            assert_eq!(log["ddtags"], "one,two,three".into());
            assert_eq!(*log.get_source_type().unwrap(), "datadog_agent".into());
            assert_eq!(
                &event.metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
            assert_eq!(
                event.metadata().schema_definition(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[test]
fn test_config_outputs_with_disabled_data_types() {
    struct TestCase {
        multiple_outputs: bool,
        disable_logs: bool,
        disable_metrics: bool,
        disable_traces: bool,
    }

    for TestCase {
        multiple_outputs,
        disable_logs,
        disable_metrics,
        disable_traces,
    } in [
        TestCase {
            multiple_outputs: true,
            disable_logs: true,
            disable_metrics: true,
            disable_traces: true,
        },
        TestCase {
            multiple_outputs: true,
            disable_logs: true,
            disable_metrics: false,
            disable_traces: false,
        },
        TestCase {
            multiple_outputs: true,
            disable_logs: false,
            disable_metrics: true,
            disable_traces: false,
        },
        TestCase {
            multiple_outputs: true,
            disable_logs: false,
            disable_metrics: false,
            disable_traces: true,
        },
        TestCase {
            multiple_outputs: true,
            disable_logs: true,
            disable_metrics: true,
            disable_traces: false,
        },
        TestCase {
            multiple_outputs: true,
            disable_logs: false,
            disable_metrics: false,
            disable_traces: false,
        },
        TestCase {
            multiple_outputs: false,
            disable_logs: true,
            disable_metrics: true,
            disable_traces: true,
        },
    ] {
        let config = DatadogAgentConfig {
            address: "0.0.0.0:8080".parse().unwrap(),
            tls: None,
            store_api_key: true,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            acknowledgements: Default::default(),
            multiple_outputs,
            disable_logs,
            disable_metrics,
            disable_traces,
            log_namespace: Some(false),
            keepalive: Default::default(),
        };

        let outputs: Vec<DataType> = config
            .outputs(LogNamespace::Legacy)
            .into_iter()
            .map(|output| output.ty)
            .collect();
        if multiple_outputs {
            assert_eq!(outputs.contains(&DataType::Log), !disable_logs);
            assert_eq!(outputs.contains(&DataType::Trace), !disable_traces);
            assert_eq!(outputs.contains(&DataType::Metric), !disable_metrics);
        } else {
            assert!(outputs.contains(&DataType::all()));
            assert!(outputs.len() == 1);
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_config_outputs() {
    struct TestCase {
        decoding: DeserializerConfig,
        multiple_outputs: bool,
        want: HashMap<Option<&'static str>, Option<schema::Definition>>,
    }

    for (
        title,
        TestCase {
            decoding,
            multiple_outputs,
            want,
        },
    ) in [
        (
            "default decoding",
            TestCase {
                decoding: default_decoding(),
                multiple_outputs: false,
                want: HashMap::from([(
                    None,
                    Some(
                        schema::Definition::empty_legacy_namespace()
                            .with_event_field(
                                &owned_value_path!("message"),
                                Kind::bytes(),
                                Some("message"),
                            )
                            .with_event_field(
                                &owned_value_path!("status"),
                                Kind::bytes(),
                                Some("severity"),
                            )
                            .with_event_field(
                                &owned_value_path!("timestamp"),
                                Kind::timestamp(),
                                Some("timestamp"),
                            )
                            .with_event_field(
                                &owned_value_path!("hostname"),
                                Kind::bytes(),
                                Some("host"),
                            )
                            .with_event_field(
                                &owned_value_path!("service"),
                                Kind::bytes(),
                                Some("service"),
                            )
                            .with_event_field(
                                &owned_value_path!("ddsource"),
                                Kind::bytes(),
                                Some("source"),
                            )
                            .with_event_field(
                                &owned_value_path!("ddtags"),
                                Kind::bytes(),
                                Some("tags"),
                            )
                            .with_event_field(
                                &owned_value_path!("source_type"),
                                Kind::bytes(),
                                None,
                            ),
                    ),
                )]),
            },
        ),
        (
            "bytes / single output",
            TestCase {
                decoding: DeserializerConfig::Bytes,
                multiple_outputs: false,
                want: HashMap::from([(
                    None,
                    Some(
                        schema::Definition::empty_legacy_namespace()
                            .with_event_field(
                                &owned_value_path!("message"),
                                Kind::bytes(),
                                Some("message"),
                            )
                            .with_event_field(
                                &owned_value_path!("status"),
                                Kind::bytes(),
                                Some("severity"),
                            )
                            .with_event_field(
                                &owned_value_path!("timestamp"),
                                Kind::timestamp(),
                                Some("timestamp"),
                            )
                            .with_event_field(
                                &owned_value_path!("hostname"),
                                Kind::bytes(),
                                Some("host"),
                            )
                            .with_event_field(
                                &owned_value_path!("service"),
                                Kind::bytes(),
                                Some("service"),
                            )
                            .with_event_field(
                                &owned_value_path!("ddsource"),
                                Kind::bytes(),
                                Some("source"),
                            )
                            .with_event_field(
                                &owned_value_path!("ddtags"),
                                Kind::bytes(),
                                Some("tags"),
                            )
                            .with_event_field(
                                &owned_value_path!("source_type"),
                                Kind::bytes(),
                                None,
                            ),
                    ),
                )]),
            },
        ),
        (
            "bytes / multiple output",
            TestCase {
                decoding: DeserializerConfig::Bytes,
                multiple_outputs: true,
                want: HashMap::from([
                    (
                        Some(LOGS),
                        Some(
                            schema::Definition::empty_legacy_namespace()
                                .with_event_field(
                                    &owned_value_path!("message"),
                                    Kind::bytes(),
                                    Some("message"),
                                )
                                .with_event_field(
                                    &owned_value_path!("status"),
                                    Kind::bytes(),
                                    Some("severity"),
                                )
                                .with_event_field(
                                    &owned_value_path!("timestamp"),
                                    Kind::timestamp(),
                                    Some("timestamp"),
                                )
                                .with_event_field(
                                    &owned_value_path!("hostname"),
                                    Kind::bytes(),
                                    Some("host"),
                                )
                                .with_event_field(
                                    &owned_value_path!("service"),
                                    Kind::bytes(),
                                    Some("service"),
                                )
                                .with_event_field(
                                    &owned_value_path!("ddsource"),
                                    Kind::bytes(),
                                    Some("source"),
                                )
                                .with_event_field(
                                    &owned_value_path!("ddtags"),
                                    Kind::bytes(),
                                    Some("tags"),
                                )
                                .with_event_field(
                                    &owned_value_path!("source_type"),
                                    Kind::bytes(),
                                    None,
                                ),
                        ),
                    ),
                    (Some(METRICS), None),
                    (Some(TRACES), None),
                ]),
            },
        ),
        (
            "json / single output",
            TestCase {
                decoding: DeserializerConfig::Json(Default::default()),
                multiple_outputs: false,
                want: HashMap::from([(
                    None,
                    Some(
                        schema::Definition::empty_legacy_namespace()
                            .with_event_field(
                                &owned_value_path!("timestamp"),
                                Kind::json().or_timestamp(),
                                None,
                            )
                            .with_event_field(&owned_value_path!("source_type"), Kind::json(), None)
                            .with_event_field(&owned_value_path!("ddsource"), Kind::json(), None)
                            .with_event_field(&owned_value_path!("ddtags"), Kind::json(), None)
                            .with_event_field(&owned_value_path!("hostname"), Kind::json(), None)
                            .with_event_field(&owned_value_path!("service"), Kind::json(), None)
                            .with_event_field(&owned_value_path!("status"), Kind::json(), None)
                            .unknown_fields(Kind::json()),
                    ),
                )]),
            },
        ),
        (
            "json / multiple output",
            TestCase {
                decoding: DeserializerConfig::Json(Default::default()),
                multiple_outputs: true,
                want: HashMap::from([
                    (
                        Some(LOGS),
                        Some(
                            schema::Definition::empty_legacy_namespace()
                                .with_event_field(
                                    &owned_value_path!("timestamp"),
                                    Kind::json().or_timestamp(),
                                    None,
                                )
                                .with_event_field(
                                    &owned_value_path!("source_type"),
                                    Kind::json(),
                                    None,
                                )
                                .with_event_field(
                                    &owned_value_path!("ddsource"),
                                    Kind::json(),
                                    None,
                                )
                                .with_event_field(&owned_value_path!("ddtags"), Kind::json(), None)
                                .with_event_field(
                                    &owned_value_path!("hostname"),
                                    Kind::json(),
                                    None,
                                )
                                .with_event_field(&owned_value_path!("service"), Kind::json(), None)
                                .with_event_field(&owned_value_path!("status"), Kind::json(), None)
                                .unknown_fields(Kind::json()),
                        ),
                    ),
                    (Some(METRICS), None),
                    (Some(TRACES), None),
                ]),
            },
        ),
        #[cfg(feature = "codecs-syslog")]
        (
            "syslog / single output",
            TestCase {
                decoding: DeserializerConfig::Syslog(Default::default()),
                multiple_outputs: false,
                want: HashMap::from([(
                    None,
                    Some(
                        schema::Definition::empty_legacy_namespace()
                            .with_event_field(
                                &owned_value_path!("message"),
                                Kind::bytes(),
                                Some("message"),
                            )
                            .with_event_field(
                                &owned_value_path!("timestamp"),
                                Kind::timestamp(),
                                Some("timestamp"),
                            )
                            .with_event_field(
                                &owned_value_path!("hostname"),
                                Kind::bytes(),
                                Some("host"),
                            )
                            .optional_field(
                                &owned_value_path!("severity"),
                                Kind::bytes(),
                                Some("severity"),
                            )
                            .optional_field(&owned_value_path!("facility"), Kind::bytes(), None)
                            .optional_field(&owned_value_path!("version"), Kind::integer(), None)
                            .optional_field(
                                &owned_value_path!("appname"),
                                Kind::bytes(),
                                Some("service"),
                            )
                            .optional_field(&owned_value_path!("msgid"), Kind::bytes(), None)
                            .optional_field(
                                &owned_value_path!("procid"),
                                Kind::integer().or_bytes(),
                                None,
                            )
                            .with_event_field(
                                &owned_value_path!("source_type"),
                                Kind::bytes().or_object(Collection::from_unknown(Kind::bytes())),
                                None,
                            )
                            .with_event_field(
                                &owned_value_path!("ddsource"),
                                Kind::bytes().or_object(Collection::from_unknown(Kind::bytes())),
                                None,
                            )
                            .with_event_field(
                                &owned_value_path!("ddtags"),
                                Kind::bytes().or_object(Collection::from_unknown(Kind::bytes())),
                                None,
                            )
                            .with_event_field(
                                &owned_value_path!("service"),
                                Kind::bytes().or_object(Collection::from_unknown(Kind::bytes())),
                                None,
                            )
                            .with_event_field(
                                &owned_value_path!("status"),
                                Kind::bytes().or_object(Collection::from_unknown(Kind::bytes())),
                                None,
                            )
                            .unknown_fields(Kind::object(
                                vrl::value::kind::Collection::from_unknown(Kind::bytes()),
                            )),
                    ),
                )]),
            },
        ),
        #[cfg(feature = "codecs-syslog")]
        (
            "syslog / multiple output",
            TestCase {
                decoding: DeserializerConfig::Syslog(Default::default()),
                multiple_outputs: true,
                want: HashMap::from([
                    (
                        Some(LOGS),
                        Some(
                            schema::Definition::empty_legacy_namespace()
                                .with_event_field(
                                    &owned_value_path!("message"),
                                    Kind::bytes(),
                                    Some("message"),
                                )
                                .with_event_field(
                                    &owned_value_path!("timestamp"),
                                    Kind::timestamp(),
                                    Some("timestamp"),
                                )
                                .with_event_field(
                                    &owned_value_path!("hostname"),
                                    Kind::bytes(),
                                    Some("host"),
                                )
                                .optional_field(
                                    &owned_value_path!("severity"),
                                    Kind::bytes(),
                                    Some("severity"),
                                )
                                .optional_field(&owned_value_path!("facility"), Kind::bytes(), None)
                                .optional_field(
                                    &owned_value_path!("version"),
                                    Kind::integer(),
                                    None,
                                )
                                .optional_field(
                                    &owned_value_path!("appname"),
                                    Kind::bytes(),
                                    Some("service"),
                                )
                                .optional_field(&owned_value_path!("msgid"), Kind::bytes(), None)
                                .optional_field(
                                    &owned_value_path!("procid"),
                                    Kind::integer().or_bytes(),
                                    None,
                                )
                                .with_event_field(
                                    &owned_value_path!("source_type"),
                                    Kind::bytes()
                                        .or_object(Collection::from_unknown(Kind::bytes())),
                                    None,
                                )
                                .with_event_field(
                                    &owned_value_path!("ddsource"),
                                    Kind::bytes()
                                        .or_object(Collection::from_unknown(Kind::bytes())),
                                    None,
                                )
                                .with_event_field(
                                    &owned_value_path!("ddtags"),
                                    Kind::bytes()
                                        .or_object(Collection::from_unknown(Kind::bytes())),
                                    None,
                                )
                                .with_event_field(
                                    &owned_value_path!("service"),
                                    Kind::bytes()
                                        .or_object(Collection::from_unknown(Kind::bytes())),
                                    None,
                                )
                                .with_event_field(
                                    &owned_value_path!("status"),
                                    Kind::bytes()
                                        .or_object(Collection::from_unknown(Kind::bytes())),
                                    None,
                                )
                                .unknown_fields(Kind::object(
                                    vrl::value::kind::Collection::from_unknown(Kind::bytes()),
                                )),
                        ),
                    ),
                    (Some(METRICS), None),
                    (Some(TRACES), None),
                ]),
            },
        ),
    ] {
        let config = DatadogAgentConfig {
            address: "0.0.0.0:8080".parse().unwrap(),
            tls: None,
            store_api_key: true,
            framing: default_framing_message_based(),
            decoding,
            acknowledgements: Default::default(),
            multiple_outputs,
            disable_logs: false,
            disable_metrics: false,
            disable_traces: false,
            log_namespace: Some(false),
            keepalive: Default::default(),
        };

        let mut outputs = config
            .outputs(LogNamespace::Legacy)
            .into_iter()
            .map(|output| (output.port.clone(), output.schema_definition(true)))
            .collect::<HashMap<_, _>>();

        for (name, want) in want {
            let got = outputs
                .remove(&name.map(ToOwned::to_owned))
                .expect("output exists");

            assert_eq!(got, want, "{}", title);
        }
    }
}

#[tokio::test]
async fn decode_series_endpoint_v2() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr) = source(EventStatus::Delivered, true, true, false).await;

        let mut headers = HeaderMap::new();
        headers.insert(
            "dd-api-key",
            "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
        );

        let series = vec![
            ddmetric_proto::metric_payload::MetricSeries {
                resources: vec![ddmetric_proto::metric_payload::Resource {
                    r#type: "host".to_string(),
                    name: "random_host".to_string(),
                }],
                metric: "namespace.dd_gauge".to_string(),
                tags: vec!["foo:bar".to_string()],
                points: vec![
                    ddmetric_proto::metric_payload::MetricPoint {
                        value: 3.14,
                        timestamp: 1542182950,
                    },
                    ddmetric_proto::metric_payload::MetricPoint {
                        value: 3.1415,
                        timestamp: 1542182951,
                    },
                ],
                r#type: ddmetric_proto::metric_payload::MetricType::Gauge as i32,
                unit: "".to_string(),
                source_type_name: "a_random_source_type_name".to_string(),
                interval: 10, // Dogstatsd sets Gauge interval to 10 by default
                metadata: None,
            },
            ddmetric_proto::metric_payload::MetricSeries {
                resources: vec![ddmetric_proto::metric_payload::Resource {
                    r#type: "host".to_string(),
                    name: "another_random_host".to_string(),
                }],
                metric: "another_namespace.dd_rate".to_string(),
                tags: vec!["foo:bar:baz".to_string(), "foo:bizbaz".to_string()],
                points: vec![ddmetric_proto::metric_payload::MetricPoint {
                    value: 3.14,
                    timestamp: 1542182950,
                }],
                r#type: ddmetric_proto::metric_payload::MetricType::Rate as i32,
                unit: "".to_string(),
                source_type_name: "another_random_source_type_name".to_string(),
                interval: 10,
                metadata: None,
            },
            ddmetric_proto::metric_payload::MetricSeries {
                resources: vec![ddmetric_proto::metric_payload::Resource {
                    r#type: "host".to_string(),
                    name: "a_host".to_string(),
                }],
                metric: "dd_count".to_string(),
                tags: vec!["foobar".to_string()],
                points: vec![ddmetric_proto::metric_payload::MetricPoint {
                    value: 16777216_f64,
                    timestamp: 1542182955,
                }],
                r#type: ddmetric_proto::metric_payload::MetricType::Count as i32,
                unit: "".to_string(),
                source_type_name: "a_very_random_source_type_name".to_string(),
                interval: 0,
                metadata: Some(ddmetric_proto::Metadata {
                    origin: Some(ddmetric_proto::Origin {
                        origin_product: 10,
                        origin_category: 10,
                        origin_service: 42,
                    }),
                }),
            },
        ];

        let series_payload = ddmetric_proto::MetricPayload { series };

        let mut buf = Vec::new();
        series_payload.encode(&mut buf).unwrap();

        let events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        unsafe { str::from_utf8_unchecked(&buf) },
                        headers,
                        "/api/v2/series"
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
                Some(
                    Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                        .single()
                        .expect("invalid timestamp")
                )
            );
            assert_eq!(metric.kind(), MetricKind::Absolute);
            assert_eq!(
                metric
                    .interval_ms()
                    .expect("should have set interval")
                    .get(),
                10000
            );
            assert_eq!(*metric.value(), MetricValue::Gauge { value: 3.14 });
            assert_tags(
                metric,
                metric_tags!(
                    "host" => "random_host",
                    "foo" => "bar",
                    "source_type_name" => "a_random_source_type_name",
                ),
            );
            assert_eq!(metric.namespace(), Some("namespace"));

            assert_eq!(
                &events[0].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            metric = events[1].as_metric();
            assert_eq!(metric.name(), "dd_gauge");
            assert_eq!(
                metric.timestamp(),
                Some(Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 11).unwrap())
            );
            assert_eq!(metric.kind(), MetricKind::Absolute);
            assert_eq!(*metric.value(), MetricValue::Gauge { value: 3.1415 });
            assert_eq!(
                metric
                    .interval_ms()
                    .expect("should have set interval")
                    .get(),
                10000
            );
            assert_tags(
                metric,
                metric_tags!(
                    "host" => "random_host",
                    "foo" => "bar",
                    "source_type_name" => "a_random_source_type_name",
                ),
            );
            assert_eq!(metric.namespace(), Some("namespace"));

            assert_eq!(
                &events[1].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            metric = events[2].as_metric();
            assert_eq!(metric.name(), "dd_rate");
            assert_eq!(
                metric.timestamp(),
                Some(
                    Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                        .single()
                        .expect("invalid timestamp")
                )
            );
            assert_eq!(metric.kind(), MetricKind::Incremental);
            assert_eq!(
                *metric.value(),
                MetricValue::Counter {
                    value: 3.14 * (10_f64)
                }
            );
            assert_tags(
                metric,
                metric_tags!(
                    "host" => "another_random_host",
                    "foo" => "bar:baz",
                    "foo" => "bizbaz",
                    "source_type_name" => "another_random_source_type_name",
                ),
            );
            assert_eq!(metric.namespace(), Some("another_namespace"));

            assert_eq!(
                &events[2].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            metric = events[3].as_metric();
            assert_eq!(metric.name(), "dd_count");
            assert_eq!(
                metric.timestamp(),
                Some(
                    Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 15)
                        .single()
                        .expect("invalid timestamp")
                )
            );
            assert_eq!(metric.kind(), MetricKind::Incremental);
            assert_eq!(
                *metric.value(),
                MetricValue::Counter {
                    value: 16777216_f64
                }
            );
            assert_tags(
                metric,
                metric_tags!(
                    "host" => "a_host",
                    "foobar" => TagValue::Bare,
                    "source_type_name" => "a_very_random_source_type_name",
                ),
            );
            assert_eq!(metric.namespace(), None);

            assert_eq!(
                &events[3].metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );

            assert_eq!(
                events[3]
                    .metadata()
                    .datadog_origin_metadata()
                    .unwrap()
                    .product()
                    .unwrap(),
                10
            );
            assert_eq!(
                events[3]
                    .metadata()
                    .datadog_origin_metadata()
                    .unwrap()
                    .category()
                    .unwrap(),
                10
            );
            assert_eq!(
                events[3]
                    .metadata()
                    .datadog_origin_metadata()
                    .unwrap()
                    .service()
                    .unwrap(),
                42
            );
        }
    })
    .await;
}

#[test]
fn test_output_schema_definition_json_vector_namespace() {
    let definition = toml::from_str::<DatadogAgentConfig>(indoc! { r#"
            address = "0.0.0.0:8012"
            decoding.codec = "json"
        "#})
    .unwrap()
    .outputs(LogNamespace::Vector)
    .remove(0)
    .schema_definition(true);

    assert_eq!(
        definition,
        Some(
            Definition::new_with_default_metadata(Kind::json(), [LogNamespace::Vector])
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "ddsource"),
                    Kind::bytes(),
                    Some("source")
                )
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "ddtags"),
                    Kind::bytes(),
                    Some("tags")
                )
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "hostname"),
                    Kind::bytes(),
                    Some("host")
                )
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "service"),
                    Kind::bytes(),
                    Some("service")
                )
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "status"),
                    Kind::bytes(),
                    Some("severity")
                )
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "timestamp"),
                    Kind::timestamp(),
                    Some("timestamp")
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None
                )
        )
    )
}

#[test]
fn test_output_schema_definition_bytes_vector_namespace() {
    let definition = toml::from_str::<DatadogAgentConfig>(indoc! { r#"
            address = "0.0.0.0:8012"
            decoding.codec = "bytes"
        "#})
    .unwrap()
    .outputs(LogNamespace::Vector)
    .remove(0)
    .schema_definition(true);

    assert_eq!(
        definition,
        Some(
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "ddsource"),
                    Kind::bytes(),
                    Some("source")
                )
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "ddtags"),
                    Kind::bytes(),
                    Some("tags")
                )
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "hostname"),
                    Kind::bytes(),
                    Some("host")
                )
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "service"),
                    Kind::bytes(),
                    Some("service")
                )
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "status"),
                    Kind::bytes(),
                    Some("severity")
                )
                .with_metadata_field(
                    &owned_value_path!("datadog_agent", "timestamp"),
                    Kind::timestamp(),
                    Some("timestamp")
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None
                )
                .with_meaning(OwnedTargetPath::event_root(), "message")
        )
    )
}

#[test]
fn test_output_schema_definition_json_legacy_namespace() {
    let definition = toml::from_str::<DatadogAgentConfig>(indoc! { r#"
            address = "0.0.0.0:8012"
            decoding.codec = "json"
        "#})
    .unwrap()
    .outputs(LogNamespace::Legacy)
    .remove(0)
    .schema_definition(true);

    assert_eq!(
        definition,
        Some(
            Definition::new_with_default_metadata(Kind::json(), [LogNamespace::Legacy])
                .with_event_field(
                    &owned_value_path!("timestamp"),
                    Kind::json().or_timestamp(),
                    None
                )
                .with_event_field(&owned_value_path!("ddsource"), Kind::json(), None)
                .with_event_field(&owned_value_path!("ddtags"), Kind::json(), None)
                .with_event_field(&owned_value_path!("hostname"), Kind::json(), None)
                .with_event_field(&owned_value_path!("service"), Kind::json(), None)
                .with_event_field(&owned_value_path!("source_type"), Kind::json(), None)
                .with_event_field(&owned_value_path!("status"), Kind::json(), None)
        )
    )
}

#[test]
fn test_output_schema_definition_bytes_legacy_namespace() {
    let definition = toml::from_str::<DatadogAgentConfig>(indoc! { r#"
            address = "0.0.0.0:8012"
            decoding.codec = "bytes"
        "#})
    .unwrap()
    .outputs(LogNamespace::Legacy)
    .remove(0)
    .schema_definition(true);

    assert_eq!(
        definition,
        Some(
            Definition::new_with_default_metadata(
                Kind::object(Collection::empty()),
                [LogNamespace::Legacy]
            )
            .with_event_field(
                &owned_value_path!("ddsource"),
                Kind::bytes(),
                Some("source")
            )
            .with_event_field(&owned_value_path!("ddtags"), Kind::bytes(), Some("tags"))
            .with_event_field(&owned_value_path!("hostname"), Kind::bytes(), Some("host"))
            .with_event_field(
                &owned_value_path!("message"),
                Kind::bytes(),
                Some("message")
            )
            .with_event_field(
                &owned_value_path!("service"),
                Kind::bytes(),
                Some("service")
            )
            .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
            .with_event_field(
                &owned_value_path!("status"),
                Kind::bytes(),
                Some("severity")
            )
            .with_event_field(
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some("timestamp")
            )
        )
    )
}

fn assert_tags(metric: &Metric, tags: MetricTags) {
    assert_eq!(metric.tags().expect("Missing tags"), &tags);
}
