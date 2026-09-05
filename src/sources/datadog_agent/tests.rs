use std::{
    collections::{BTreeMap, HashMap},
    iter::FromIterator,
    net::SocketAddr,
    str,
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{Stream, StreamExt};
use http::HeaderMap;
use indoc::indoc;
use prost::Message;
use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
use similar_asserts::assert_eq;
use tokio::time::timeout;
use vector_lib::{
    codecs::{
        BytesDecoder, BytesDeserializer, CharacterDelimitedDecoderConfig,
        decoding::{
            BytesDeserializerConfig, CharacterDelimitedDecoderOptions, Deserializer,
            DeserializerConfig, Framer,
        },
    },
    config::{DataType, LogNamespace},
    event::{MetricTags, metric::TagValue},
    lookup::{OwnedTargetPath, owned_value_path},
    metric_tags,
};
use vrl::{compiler::value::Collection, value, value::Kind};

use crate::{
    SourceSender,
    common::datadog::{DatadogMetricType, DatadogPoint, DatadogSeriesMetric},
    components::validation::prelude::*,
    config::{SourceConfig, SourceContext},
    event::{
        Event, EventStatus, Metric, Value, into_event_stream,
        metric::{MetricKind, MetricSketch, MetricValue},
    },
    metrics::Controller,
    schema,
    schema::Definition,
    serde::{default_decoding, default_framing_message_based},
    sources::datadog_agent::{
        DatadogAgentConfig, DatadogAgentSource, LOGS, LogMsg, METRICS, TRACES, ddmetric_proto,
        ddtrace_proto, logs::decode_log_body, metrics::DatadogSeriesRequest,
    },
    test_util::{
        addr::{PortGuard, next_addr},
        components::{
            COMPONENT_ERROR_TAGS, HTTP_PUSH_SOURCE_TAGS, assert_source_compliance,
            assert_source_error,
        },
        spawn_collect_n, trace_init, wait_for_tcp,
    },
};

use crate::sources::datadog_agent::llmobs::decode_llmobs_body;

const DD_API_KEY: &str = "12345678abcdefgh12345678abcdefgh";
const DD_API_LOGS_V1_PATH: &str = "/v1/input/";
const DD_API_LOGS_V2_PATH: &str = "/api/v2/logs";
const DD_API_SERIES_V1_PATH: &str = "/api/v1/series";
const DD_API_SERIES_V2_PATH: &str = "/api/v2/series";
const DD_API_SKETCHES_PATH: &str = "/api/beta/sketches";
const DD_API_TRACES_PATH: &str = "/api/v0.2/traces";
const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

fn make_llmobs_source() -> DatadogAgentSource {
    let decoder = vector_lib::codecs::Decoder::new(
        Framer::Bytes(BytesDecoder::new()),
        Deserializer::Bytes(BytesDeserializer),
    );
    DatadogAgentSource::new(
        true,
        decoder,
        "http",
        None,
        LogNamespace::Legacy,
        false,
        true,
    )
}

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
        let decoder = vector_lib::codecs::Decoder::new(
            Framer::Bytes(BytesDecoder::new()),
            Deserializer::Bytes(BytesDeserializer),
        );

        let source = DatadogAgentSource::new(
            true,
            decoder,
            "http",
            Some(test_logs_schema_definition()),
            LogNamespace::Legacy,
            false,
            true,
        );

        let events = decode_log_body(body, api_key, &source).unwrap();
        assert_eq!(events.len(), msgs.len());
        for (msg, event) in msgs.into_iter().zip(events) {
            let log = event.as_log();
            assert_eq!(log["message"], msg.message.into());
            assert_eq!(log["status"], msg.status.into());
            assert_eq!(log["timestamp"], msg.timestamp.into());
            assert_eq!(log["hostname"], msg.hostname.into());
            assert_eq!(log["service"], msg.service.into());
            assert_eq!(log["ddsource"], msg.ddsource.into());
            assert_eq!(log["ddtags"], msg.ddtags.into());

            assert_eq!(
                event.metadata().schema_definition().as_ref(),
                &test_logs_schema_definition()
            );
        }

        TestResult::passed()
    }

    QuickCheck::new().quickcheck(inner as fn(Vec<LogMsg>) -> TestResult);
}

#[test]
fn test_decode_log_body_parse_ddtags() {
    let log_msgs = [LogMsg {
        message: Bytes::from(String::from("message")),
        status: Bytes::from(String::from("status")),
        timestamp: Utc
            .timestamp_millis_opt(1234)
            .single()
            .expect("invalid timestamp"),
        hostname: Bytes::from(String::from("host")),
        service: Bytes::from(String::from("service")),
        ddsource: Bytes::from(String::from("ddsource")),
        ddtags: Bytes::from(String::from("wizard:the_grey,env:staging")),
    }];

    let body = Bytes::from(serde_json::to_string(&log_msgs).unwrap());
    let api_key = None;
    let decoder = vector_lib::codecs::Decoder::new(
        Framer::Bytes(BytesDecoder::new()),
        Deserializer::Bytes(BytesDeserializer),
    );

    let source = DatadogAgentSource::new(
        true,
        decoder,
        "http",
        Some(test_logs_schema_definition()),
        LogNamespace::Legacy,
        true,
        true,
    );

    let events = decode_log_body(body, api_key, &source).unwrap();

    assert_eq!(events.len(), 1);

    let event = events.first().unwrap();
    let log = event.as_log();
    let log_msg = log_msgs[0].clone();

    assert_eq!(log["message"], log_msg.message.into());
    assert_eq!(log["status"], log_msg.status.into());
    assert_eq!(log["timestamp"], log_msg.timestamp.into());
    assert_eq!(log["hostname"], log_msg.hostname.into());
    assert_eq!(log["service"], log_msg.service.into());
    assert_eq!(log["ddsource"], log_msg.ddsource.into());

    assert_eq!(log["ddtags"], value!(["wizard:the_grey", "env:staging"]));
}

#[test]
fn test_decode_log_body_empty_object() {
    let body = Bytes::from("{}");
    let api_key = None;
    let decoder = vector_lib::codecs::Decoder::new(
        Framer::Bytes(BytesDecoder::new()),
        Deserializer::Bytes(BytesDeserializer),
    );

    let source = DatadogAgentSource::new(
        true,
        decoder,
        "http",
        Some(test_logs_schema_definition()),
        LogNamespace::Legacy,
        false,
        true,
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
    split_metric_namespace: bool,
) -> (
    impl Stream<Item = Event> + Unpin,
    Option<impl Stream<Item = Event>>,
    Option<impl Stream<Item = Event>>,
    SocketAddr,
    PortGuard,
) {
    let (sender, recv) = SourceSender::new_test_finalize(status);
    let (logs_output, metrics_output, address, guard) = source_with_sender(
        sender,
        status,
        acknowledgements,
        store_api_key,
        multiple_outputs,
        split_metric_namespace,
    )
    .await;
    (recv, logs_output, metrics_output, address, guard)
}

async fn source_with_timeout(
    status: EventStatus,
    acknowledgements: bool,
    store_api_key: bool,
    multiple_outputs: bool,
    split_metric_namespace: bool,
    send_timeout: Duration,
) -> (
    impl Stream<Item = Event> + Unpin,
    Option<impl Stream<Item = Event>>,
    Option<impl Stream<Item = Event>>,
    SocketAddr,
    PortGuard,
) {
    let (sender, recv) = SourceSender::new_test_sender_with_options(1, Some(send_timeout));
    let (logs_output, metrics_output, address, guard) = source_with_sender(
        sender,
        status,
        acknowledgements,
        store_api_key,
        multiple_outputs,
        split_metric_namespace,
    )
    .await;
    let recv = recv.into_stream().flat_map(into_event_stream);
    (recv, logs_output, metrics_output, address, guard)
}

async fn source_with_sender(
    mut sender: SourceSender,
    status: EventStatus,
    acknowledgements: bool,
    store_api_key: bool,
    multiple_outputs: bool,
    split_metric_namespace: bool,
) -> (
    Option<impl Stream<Item = Event>>,
    Option<impl Stream<Item = Event>>,
    SocketAddr,
    PortGuard,
) {
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
    let (guard, address) = next_addr();
    let config = serde_yaml::from_str::<DatadogAgentConfig>(&format!(
        indoc! { r#"
            address: "{}"
            compression: none
            store_api_key: {}
            acknowledgements: {}
            multiple_outputs: {}
            split_metric_namespace: {}
            trace_proto: v1v2
        "#},
        address, store_api_key, acknowledgements, multiple_outputs, split_metric_namespace
    ))
    .unwrap();
    let schema_definitions =
        HashMap::from([(Some(LOGS.to_owned()), test_logs_schema_definition())]);
    let context = SourceContext::new_test(sender, Some(schema_definitions));
    tokio::spawn(async move {
        config.build(context).await.unwrap().await.unwrap();
    });
    wait_for_tcp(address).await;
    (logs_output, metrics_output, address, guard)
}

async fn send_with_path(address: SocketAddr, body: &str, headers: HeaderMap, path: &str) -> u16 {
    timeout(
        HTTP_REQUEST_TIMEOUT,
        reqwest::Client::new()
            .post(format!("http://{address}{path}"))
            .headers(headers)
            .body(body.to_owned())
            .send(),
    )
    .await
    .expect("send_with_path request timed out")
    .unwrap()
    .status()
    .as_u16()
}

async fn send_and_collect(
    address: SocketAddr,
    body: String,
    headers: HeaderMap,
    path: &'static str,
    rx: impl Stream<Item = Event> + Unpin,
    expected_count: usize,
) -> Vec<Event> {
    spawn_collect_n(
        async move {
            assert_eq!(200, send_with_path(address, &body, headers, path).await);
        },
        rx,
        expected_count,
    )
    .await
}

fn dd_api_key_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("dd-api-key", DD_API_KEY.parse().unwrap());
    headers
}

#[tokio::test]
async fn full_payload_v1() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

        let mut events = send_and_collect(
            addr,
            serde_json::to_string(&[LogMsg {
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
            DD_API_LOGS_V1_PATH,
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
                event.metadata().schema_definition().as_ref(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn full_payload_v2() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

        let mut events = send_and_collect(
            addr,
            serde_json::to_string(&[LogMsg {
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
            DD_API_LOGS_V2_PATH,
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
                event.metadata().schema_definition().as_ref(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn no_api_key() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

        let mut events = send_and_collect(
            addr,
            serde_json::to_string(&[LogMsg {
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
            DD_API_LOGS_V1_PATH,
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
                event.metadata().schema_definition().as_ref(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn api_key_in_url() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

        let mut events = send_and_collect(
            addr,
            serde_json::to_string(&[LogMsg {
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
            "/v1/input/12345678abcdefgh12345678abcdefgh",
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
                event.metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
            );
            assert_eq!(
                event.metadata().schema_definition().as_ref(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn api_key_in_query_params() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

        let mut events = send_and_collect(
            addr,
            serde_json::to_string(&[LogMsg {
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
            "/api/v2/logs?dd-api-key=12345678abcdefgh12345678abcdefgh",
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
                event.metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
            );
            assert_eq!(
                event.metadata().schema_definition().as_ref(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn api_key_in_header() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

        let mut events = send_and_collect(
            addr,
            serde_json::to_string(&[LogMsg {
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
            dd_api_key_headers(),
            DD_API_LOGS_V1_PATH,
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
                event.metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
            );
            assert_eq!(
                event.metadata().schema_definition().as_ref(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn delivery_failure() {
    trace_init();
    let (rx, _, _, addr, _guard) = source(EventStatus::Rejected, true, true, false, true).await;

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
                    DD_API_LOGS_V1_PATH
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
async fn send_timeout_returns_service_unavailable() {
    trace_init();
    let (rx, _, _, addr, _guard) = source_with_timeout(
        EventStatus::Delivered,
        false,
        true,
        false,
        true,
        Duration::from_millis(50),
    )
    .await;

    let body = serde_json::to_string(&[LogMsg {
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
    .unwrap();

    assert_eq!(
        200,
        send_with_path(addr, &body, HeaderMap::new(), DD_API_LOGS_V1_PATH).await
    );

    assert_eq!(
        503,
        send_with_path(addr, &body, HeaderMap::new(), DD_API_LOGS_V1_PATH).await
    );
    drop(rx);
}

#[test]
fn parse_config_with_send_timeout_secs() {
    let config = serde_yaml::from_str::<DatadogAgentConfig>(indoc! { r#"
            address: "0.0.0.0:8012"
            send_timeout_secs: 1.5
        "#})
    .unwrap();

    assert_eq!(config.send_timeout_secs, Some(1.5));
    assert_eq!(config.send_timeout(), Some(Duration::from_secs_f64(1.5)));
}

#[test]
fn parse_config_without_send_timeout_secs() {
    let config = serde_yaml::from_str::<DatadogAgentConfig>(indoc! { r#"
            address: "0.0.0.0:8012"
        "#})
    .unwrap();

    assert_eq!(config.send_timeout_secs, None);
    assert_eq!(config.send_timeout(), None);
}

#[tokio::test]
async fn ignores_disabled_acknowledgements() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Rejected, false, true, false, true).await;

        let events = send_and_collect(
            addr,
            serde_json::to_string(&[LogMsg {
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
            DD_API_LOGS_V1_PATH,
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
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, false, false, true).await;

        let mut events = send_and_collect(
            addr,
            serde_json::to_string(&[LogMsg {
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
            dd_api_key_headers(),
            "/v1/input/12345678abcdefgh12345678abcdefgh",
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
                event.metadata().schema_definition().as_ref(),
                &test_logs_schema_definition()
            );
        }
    })
    .await;
}

#[tokio::test]
async fn decode_series_endpoint_v1() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

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
        let events = send_and_collect(
            addr,
            serde_json::to_string(&dd_metric_request).unwrap(),
            dd_api_key_headers(),
            DD_API_SERIES_V1_PATH,
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
                events[0].metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
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
                events[1].metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
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
                events[2].metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
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
                events[3].metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
            );
        }
    })
    .await;
}

#[tokio::test]
async fn decode_sketches() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

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
        let body = unsafe { String::from_utf8_unchecked(buf) };
        let events = send_and_collect(
            addr,
            body,
            dd_api_key_headers(),
            DD_API_SKETCHES_PATH,
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
                events[0].metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
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
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

        let headers = dd_api_key_headers();

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
            ..Default::default()
        };

        let mut buf = Vec::new();

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
            ..Default::default()
        };

        let payload = ddtrace_proto::AgentPayload {
            host_name: "a_hostname".to_string(),
            env: "env".to_string(),
            tracer_payloads: vec![tracer_payload],
            tags: BTreeMap::new(),
            agent_version: "v1.23456".to_string(),
            target_tps: 10f64,
            error_tps: 10f64,
            ..Default::default()
        };

        payload.encode(&mut buf).unwrap();

        let events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        unsafe { str::from_utf8_unchecked(&buf) },
                        headers,
                        DD_API_TRACES_PATH
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let trace = events[0].as_trace();
            assert_eq!(
                events[0].metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
            );
            let start = Value::from(Utc.timestamp_nanos(1_431_648_000_000_001i64));
            assert_eq!(
                Value::Object(trace.as_map().clone()),
                value!({
                    "host": "a_hostname",
                    "env": "env",
                    "source_type": "datadog_agent",
                    "payload_version": "v2",
                    "agent_version": "v1.23456",
                    "target_tps": 10.0,
                    "error_tps": 10.0,
                    "priority": 42,
                    "origin": "an_origin",
                    "dropped": false,
                    "tags": {
                        "a": "tag",
                        "another": "tag"
                    },
                    "container_id": "an_id",
                    "language_name": "plop",
                    "language_version": "v33",
                    "tracer_version": "v577",
                    "runtime_id": "123abc",
                    "app_version": "v314",
                    "spans": [{
                        "service": "a_service",
                        "name": "a_name",
                        "resource": "a_resource",
                        "trace_id": 123,
                        "span_id": 456,
                        "parent_id": 789,
                        "start": start,
                        "duration": 1_000_000_000,
                        "error": 404,
                        "meta": { "foo": "bar" },
                        "metrics": { "a_metrics": 0.577 },
                        "type": "a_type",
                        "meta_struct": {},
                        "span_links": [],
                        "span_events": []
                    }]
                })
            );
        }
    })
    .await;
}

#[tokio::test]
async fn decode_traces_span_links_and_events() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

        let headers = dd_api_key_headers();
        let mut buf = Vec::new();

        let span = ddtrace_proto::Span {
            service: "a_service".to_string(),
            name: "a_name".to_string(),
            resource: "a_resource".to_string(),
            trace_id: 123u64,
            span_id: 456u64,
            span_links: vec![ddtrace_proto::SpanLink {
                trace_id: u64::MAX,
                trace_id_high: 1u64 << 63,
                span_id: 0xdead_beef_cafe_babe,
                attributes: BTreeMap::from([("link".to_string(), "yes".to_string())]),
                tracestate: "vendor=1".to_string(),
                flags: 1,
            }],
            span_events: vec![ddtrace_proto::SpanEvent {
                time_unix_nano: 1_431_648_000_000_001,
                name: "exception".to_string(),
                attributes: BTreeMap::from([(
                    "exception.message".to_string(),
                    ddtrace_proto::AttributeAnyValue {
                        r#type:
                            ddtrace_proto::attribute_any_value::AttributeAnyValueType::StringValue
                                as i32,
                        string_value: "boom".to_string(),
                        ..Default::default()
                    },
                )]),
            }],
            ..Default::default()
        };

        ddtrace_proto::AgentPayload {
            tracer_payloads: vec![ddtrace_proto::TracerPayload {
                chunks: vec![ddtrace_proto::TraceChunk {
                    spans: vec![span],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }
        .encode(&mut buf)
        .unwrap();

        let events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        unsafe { str::from_utf8_unchecked(&buf) },
                        headers,
                        DD_API_TRACES_PATH
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        let event_time = Value::from(Utc.timestamp_nanos(1_431_648_000_000_001i64));
        let start = Value::from(Utc.timestamp_nanos(0));
        assert_eq!(
            Value::Object(events[0].as_trace().as_map().clone()),
            value!({
                "host": "",
                "env": "",
                "source_type": "datadog_agent",
                "payload_version": "v2",
                "agent_version": "",
                "target_tps": 0.0,
                "error_tps": 0.0,
                "priority": 0,
                "origin": "",
                "dropped": false,
                "tags": {},
                "container_id": "",
                "language_name": "",
                "language_version": "",
                "tracer_version": "",
                "runtime_id": "",
                "app_version": "",
                "spans": [{
                    "service": "a_service",
                    "name": "a_name",
                    "resource": "a_resource",
                    "trace_id": 123,
                    "span_id": 456,
                    "parent_id": 0,
                    "start": start,
                    "duration": 0,
                    "error": 0,
                    "meta": {},
                    "metrics": {},
                    "type": "",
                    "meta_struct": {},
                    "span_links": [{
                        "trace_id": "ffffffffffffffff",
                        "trace_id_high": "8000000000000000",
                        "span_id": "deadbeefcafebabe",
                        "attributes": { "link": "yes" },
                        "tracestate": "vendor=1",
                        "flags": 1
                    }],
                    "span_events": [{
                        "time_unix_nano": event_time,
                        "name": "exception",
                        "attributes": { "exception.message": "boom" }
                    }]
                }]
            })
        );
    })
    .await;
}

#[tokio::test]
async fn decode_traces_empty_tracer_payloads_emits_error() {
    crate::test_util::components::init_test();
    assert_source_error(&COMPONENT_ERROR_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

        let mut buf = Vec::new();
        ddtrace_proto::AgentPayload::default()
            .encode(&mut buf)
            .unwrap();

        assert_eq!(
            200,
            send_with_path(
                addr,
                unsafe { str::from_utf8_unchecked(&buf) },
                dd_api_key_headers(),
                DD_API_TRACES_PATH
            )
            .await
        );

        let events = crate::test_util::collect_ready(rx).await;
        assert!(events.is_empty());

        let metrics = Controller::get().unwrap().capture_metrics();
        let errors = metrics
            .iter()
            .find(|m| m.name() == "component_errors_total")
            .expect("component_errors_total should be present");
        match errors.value() {
            crate::event::metric::MetricValue::Counter { value } => {
                assert!(*value >= 1.0, "expected at least one component error");
            }
            other => panic!("unexpected metric value {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn decode_traces_idx_only_payload_emits_error() {
    crate::test_util::components::init_test();
    assert_source_error(&COMPONENT_ERROR_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

        let mut buf = Vec::new();
        ddtrace_proto::AgentPayload {
            idx_tracer_payloads: vec![ddtrace_proto::idx::TracerPayload::default()],
            ..Default::default()
        }
        .encode(&mut buf)
        .unwrap();

        assert_eq!(
            200,
            send_with_path(
                addr,
                unsafe { str::from_utf8_unchecked(&buf) },
                dd_api_key_headers(),
                DD_API_TRACES_PATH
            )
            .await
        );

        let events = crate::test_util::collect_ready(rx).await;
        assert!(events.is_empty());

        let metrics = Controller::get().unwrap().capture_metrics();
        assert!(
            metrics.iter().any(|m| {
                m.name() == "component_errors_total"
                    && m.tag_matches("error_code", "idx_tracer_payloads")
            }),
            "expected component_errors_total with error_code=idx_tracer_payloads"
        );
    })
    .await;
}

#[tokio::test]
async fn split_outputs() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (_, rx_logs, rx_metrics, addr, _guard) =
            source(EventStatus::Delivered, true, true, true, true).await;

        let mut log_event = send_and_collect(
            addr,
            serde_json::to_string(&[LogMsg {
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
            dd_api_key_headers(),
            DD_API_LOGS_V1_PATH,
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
        let mut metric_event = send_and_collect(
            addr,
            serde_json::to_string(&dd_metric_request).unwrap(),
            headers_for_metric,
            DD_API_SERIES_V1_PATH,
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
                event.metadata().datadog_api_key().as_deref().unwrap(),
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
                event.metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
            );
            assert_eq!(
                event.metadata().schema_definition().as_ref(),
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
        disable_llmobs: bool,
    }

    for TestCase {
        multiple_outputs,
        disable_logs,
        disable_metrics,
        disable_traces,
        disable_llmobs,
    } in [
        TestCase {
            multiple_outputs: true,
            disable_logs: true,
            disable_metrics: true,
            disable_traces: true,
            disable_llmobs: false,
        },
        TestCase {
            multiple_outputs: true,
            disable_logs: true,
            disable_metrics: false,
            disable_traces: false,
            disable_llmobs: false,
        },
        TestCase {
            multiple_outputs: true,
            disable_logs: false,
            disable_metrics: true,
            disable_traces: false,
            disable_llmobs: false,
        },
        TestCase {
            multiple_outputs: true,
            disable_logs: false,
            disable_metrics: false,
            disable_traces: true,
            disable_llmobs: false,
        },
        TestCase {
            multiple_outputs: true,
            disable_logs: true,
            disable_metrics: true,
            disable_traces: false,
            disable_llmobs: false,
        },
        TestCase {
            multiple_outputs: true,
            disable_logs: false,
            disable_metrics: false,
            disable_traces: false,
            disable_llmobs: false,
        },
        TestCase {
            multiple_outputs: false,
            disable_logs: true,
            disable_metrics: true,
            disable_traces: true,
            disable_llmobs: false,
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
            disable_llmobs,
            parse_ddtags: false,
            split_metric_namespace: true,
            log_namespace: Some(false),
            keepalive: Default::default(),
            send_timeout_secs: None,
        };

        let outputs: Vec<DataType> = config
            .outputs(LogNamespace::Legacy)
            .into_iter()
            .map(|output| output.ty)
            .collect();
        if multiple_outputs {
            assert_eq!(
                outputs.contains(&DataType::Log),
                !disable_logs || !disable_llmobs
            );
            assert_eq!(outputs.contains(&DataType::Trace), !disable_traces);
            assert_eq!(outputs.contains(&DataType::Metric), !disable_metrics);
        } else {
            assert!(outputs.contains(&DataType::all_bits()));
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
            disable_llmobs: false,
            parse_ddtags: false,
            split_metric_namespace: true,
            log_namespace: Some(false),
            keepalive: Default::default(),
            send_timeout_secs: None,
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
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, true).await;

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
        let body = unsafe { String::from_utf8_unchecked(buf) };
        let events = send_and_collect(
            addr,
            body,
            dd_api_key_headers(),
            DD_API_SERIES_V2_PATH,
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
                events[0].metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
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
                events[1].metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
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
                events[2].metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
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
                events[3].metadata().datadog_api_key().as_deref().unwrap(),
                DD_API_KEY
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
    let definition = serde_yaml::from_str::<DatadogAgentConfig>(indoc! { r#"
            address: "0.0.0.0:8012"
            decoding:
              codec: json
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
    let definition = serde_yaml::from_str::<DatadogAgentConfig>(indoc! { r#"
            address: "0.0.0.0:8012"
            decoding:
              codec: bytes
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
    let definition = serde_yaml::from_str::<DatadogAgentConfig>(indoc! { r#"
            address: "0.0.0.0:8012"
            decoding:
              codec: json
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
    let definition = serde_yaml::from_str::<DatadogAgentConfig>(indoc! { r#"
            address: "0.0.0.0:8012"
            decoding:
              codec: bytes
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

async fn test_series_v1_split_metric_namespace_impl(
    split: bool,
    expected_name: &str,
    expected_namespace: Option<&str>,
) {
    let (rx, _, _, addr, _guard) = source(EventStatus::Delivered, true, true, false, split).await;

    let dd_metric_request = DatadogSeriesRequest {
        series: vec![DatadogSeriesMetric {
            metric: "system.disk.free".to_string(),
            r#type: DatadogMetricType::Gauge,
            interval: None,
            points: vec![DatadogPoint(1542182950, 42.0)],
            tags: Some(vec!["foo:bar".to_string()]),
            host: Some("test_host".to_string()),
            source_type_name: None,
            device: None,
            metadata: None,
        }],
    };

    let events = send_and_collect(
        addr,
        serde_json::to_string(&dd_metric_request).unwrap(),
        dd_api_key_headers(),
        DD_API_SERIES_V1_PATH,
        rx,
        1,
    )
    .await;

    let metric = events[0].as_metric();
    assert_eq!(metric.name(), expected_name);
    assert_eq!(metric.namespace(), expected_namespace);
}

#[tokio::test]
async fn series_v1_split_metric_namespace_true() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        test_series_v1_split_metric_namespace_impl(true, "disk.free", Some("system")).await;
    })
    .await;
}

#[tokio::test]
async fn series_v1_split_metric_namespace_false() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        test_series_v1_split_metric_namespace_impl(false, "system.disk.free", None).await;
    })
    .await;
}

async fn test_series_v2_split_metric_namespace_impl(
    split: bool,
    expected_name: &str,
    expected_namespace: Option<&str>,
) {
    let (rx, _, _, addr, _guard) = source(EventStatus::Delivered, true, true, false, split).await;

    let series = vec![ddmetric_proto::metric_payload::MetricSeries {
        resources: vec![ddmetric_proto::metric_payload::Resource {
            r#type: "host".to_string(),
            name: "test_host".to_string(),
        }],
        metric: "system.disk.free".to_string(),
        tags: vec!["foo:bar".to_string()],
        points: vec![ddmetric_proto::metric_payload::MetricPoint {
            value: 42.0,
            timestamp: 1542182950,
        }],
        r#type: ddmetric_proto::metric_payload::MetricType::Gauge as i32,
        unit: "".to_string(),
        source_type_name: "".to_string(),
        interval: 10,
        metadata: None,
    }];

    let series_payload = ddmetric_proto::MetricPayload { series };

    let mut buf = Vec::new();
    series_payload.encode(&mut buf).unwrap();
    let body = unsafe { String::from_utf8_unchecked(buf) };
    let events = send_and_collect(
        addr,
        body,
        dd_api_key_headers(),
        DD_API_SERIES_V2_PATH,
        rx,
        1,
    )
    .await;

    let metric = events[0].as_metric();
    assert_eq!(metric.name(), expected_name);
    assert_eq!(metric.namespace(), expected_namespace);
}

#[tokio::test]
async fn series_v2_split_metric_namespace_true() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        test_series_v2_split_metric_namespace_impl(true, "disk.free", Some("system")).await;
    })
    .await;
}

#[tokio::test]
async fn series_v2_split_metric_namespace_false() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        test_series_v2_split_metric_namespace_impl(false, "system.disk.free", None).await;
    })
    .await;
}

#[tokio::test]
async fn series_v2_resources_preserved_as_tags() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, _, _, addr, _guard) =
            source(EventStatus::Delivered, true, true, false, false).await;

        let series = vec![ddmetric_proto::metric_payload::MetricSeries {
            resources: vec![
                ddmetric_proto::metric_payload::Resource {
                    r#type: "host".to_string(),
                    name: "test_host".to_string(),
                },
                ddmetric_proto::metric_payload::Resource {
                    r#type: "device".to_string(),
                    name: "sda".to_string(),
                },
                ddmetric_proto::metric_payload::Resource {
                    r#type: "database_instance".to_string(),
                    name: "mongo-repro-01".to_string(),
                },
                ddmetric_proto::metric_payload::Resource {
                    r#type: "database_instance".to_string(),
                    name: "mongo-repro-02".to_string(),
                },
            ],
            metric: "system.disk.free".to_string(),
            tags: vec![
                "env:prod".to_string(),
                "resource.database_instance:custom".to_string(),
            ],
            points: vec![ddmetric_proto::metric_payload::MetricPoint {
                value: 100.0,
                timestamp: 1542182950,
            }],
            r#type: ddmetric_proto::metric_payload::MetricType::Gauge as i32,
            unit: "".to_string(),
            source_type_name: "".to_string(),
            interval: 0,
            metadata: None,
        }];

        let series_payload = ddmetric_proto::MetricPayload { series };
        let mut buf = Vec::new();
        series_payload.encode(&mut buf).unwrap();
        let body = unsafe { String::from_utf8_unchecked(buf) };

        let events = send_and_collect(
            addr,
            body,
            dd_api_key_headers(),
            DD_API_SERIES_V2_PATH,
            rx,
            1,
        )
        .await;

        let metric = events[0].as_metric();
        let tags = metric.tags().unwrap();

        // The `device` resource type must be preserved as a plain `device` tag,
        // NOT as `resource.device`. This matches v1 series behavior.
        assert_eq!(tags.get("device"), Some("sda"));
        assert!(
            tags.get("resource.device").is_none(),
            "device should not be prefixed with 'resource.'"
        );
        let database_instances: Vec<_> = tags
            .iter_all()
            .filter_map(|(name, value)| (name == "resource.database_instance").then_some(value))
            .collect();
        assert_eq!(
            database_instances,
            vec![
                Some("custom"),
                Some("mongo-repro-01"),
                Some("mongo-repro-02")
            ]
        );
        assert_eq!(tags.get("env"), Some("prod"));
    })
    .await;
}

async fn test_sketches_split_metric_namespace_impl(
    split: bool,
    expected_name: &str,
    expected_namespace: Option<&str>,
) {
    let (rx, _, _, addr, _guard) = source(EventStatus::Delivered, true, true, false, split).await;

    let mut buf = Vec::new();
    let sketch = ddmetric_proto::sketch_payload::Sketch {
        metric: "system.disk.free".to_string(),
        tags: vec!["foo:bar".to_string()],
        host: "test_host".to_string(),
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
        metadata: None,
    };

    let sketch_payload = ddmetric_proto::SketchPayload {
        metadata: None,
        sketches: vec![sketch],
    };

    sketch_payload.encode(&mut buf).unwrap();
    let body = unsafe { String::from_utf8_unchecked(buf) };
    let events = send_and_collect(
        addr,
        body,
        dd_api_key_headers(),
        DD_API_SKETCHES_PATH,
        rx,
        1,
    )
    .await;

    let metric = events[0].as_metric();
    assert_eq!(metric.name(), expected_name);
    assert_eq!(metric.namespace(), expected_namespace);
}

#[tokio::test]
async fn sketches_split_metric_namespace_true() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        test_sketches_split_metric_namespace_impl(true, "disk.free", Some("system")).await;
    })
    .await;
}

#[tokio::test]
async fn sketches_split_metric_namespace_false() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        test_sketches_split_metric_namespace_impl(false, "system.disk.free", None).await;
    })
    .await;
}

impl ValidatableComponent for DatadogAgentConfig {
    fn validation_configuration() -> ValidationConfiguration {
        use vector_lib::codecs::DecodingConfig;

        let config = DatadogAgentConfig {
            address: "0.0.0.0:9007".parse().unwrap(),
            tls: None,
            store_api_key: false,
            framing: CharacterDelimitedDecoderConfig {
                character_delimited: CharacterDelimitedDecoderOptions {
                    delimiter: b',',
                    max_length: Some(usize::MAX),
                    oversized_action: Default::default(),
                },
            }
            .into(),
            decoding: BytesDeserializerConfig::new().into(),
            acknowledgements: Default::default(),
            multiple_outputs: false,
            disable_logs: false,
            disable_metrics: false,
            disable_traces: false,
            disable_llmobs: false,
            parse_ddtags: false,
            split_metric_namespace: true,
            log_namespace: Some(false),
            keepalive: Default::default(),
            send_timeout_secs: None,
        };

        let log_namespace: LogNamespace = config.log_namespace.unwrap_or_default().into();

        // TODO set up separate test cases for metrics and traces endpoints

        let logs_addr = format!("http://{}/api/v2/logs", config.address);
        let uri = http::Uri::try_from(&logs_addr).expect("should not fail to parse URI");

        let decoder = DecodingConfig::new(
            config.framing.clone(),
            DeserializerConfig::Json(Default::default()),
            false.into(),
        );

        let external_resource = ExternalResource::new(
            ResourceDirection::Push,
            HttpResourceConfig::from_parts(uri, None),
            decoder,
        );

        ValidationConfiguration::from_source(
            Self::NAME,
            log_namespace,
            vec![ComponentTestCaseConfig::from_source(
                config,
                None,
                Some(external_resource),
            )],
        )
    }
}

register_validatable_component!(DatadogAgentConfig);

#[test]
fn test_decode_llmobs_body() {
    let body = Bytes::from(
        r#"[
        {
            "event_type": "span",
            "_dd.tracer_version": "2.17.0",
            "spans": [{
                "span_id": "abc123",
                "trace_id": "xyz789",
                "name": "my.workflow",
                "start_ns": 1707763310981223236,
                "duration": 12345678900,
                "status": "ok",
                "meta": { "span": { "kind": "llm" }, "model_name": "gpt-4" },
                "metrics": { "input_tokens": 64, "output_tokens": 128 },
                "tags": ["env:prod", "service:myapp"],
                "_dd": { "ml_app": "my-llm-app" }
            }]
        }
    ]"#,
    );

    let source = make_llmobs_source();
    let events = decode_llmobs_body(body, None, &source).unwrap();
    assert_eq!(events.len(), 1);

    let log = events[0].as_log();
    assert_eq!(log["span_id"], "abc123".into());
    assert_eq!(log["trace_id"], "xyz789".into());
    assert_eq!(log["name"], "my.workflow".into());
    assert_eq!(log["status"], "ok".into());
    assert_eq!(log["ml_app"], "my-llm-app".into());
    assert_eq!(
        log["_dd"].as_object().unwrap()["tracer_version"],
        "2.17.0".into()
    );
}

#[test]
fn test_decode_llmobs_body_single_envelope() {
    // Real SDK clients (e.g. dd-trace-py's `LLMObsSpanEncoder`) POST a single JSON object,
    // not a JSON array of objects.
    let body = Bytes::from(
        r#"{
            "_dd.stage": "raw",
            "event_type": "span",
            "_dd.tracer_version": "2.17.0",
            "spans": [{
                "span_id": "abc123",
                "trace_id": "xyz789",
                "name": "my.workflow",
                "start_ns": 1707763310981223236,
                "duration": 12345678900,
                "status": "ok",
                "meta": { "span": { "kind": "llm" }, "model_name": "gpt-4" },
                "metrics": { "input_tokens": 64, "output_tokens": 128 },
                "tags": ["env:prod", "service:myapp"],
                "_dd": { "ml_app": "my-llm-app" }
            }]
        }"#,
    );

    let source = make_llmobs_source();
    let events = decode_llmobs_body(body, None, &source).unwrap();
    assert_eq!(events.len(), 1);

    let log = events[0].as_log();
    assert_eq!(log["span_id"], "abc123".into());
    assert_eq!(log["trace_id"], "xyz789".into());
    assert_eq!(log["name"], "my.workflow".into());
    assert_eq!(log["status"], "ok".into());
    assert_eq!(log["ml_app"], "my-llm-app".into());
    assert_eq!(
        log["_dd"].as_object().unwrap()["tracer_version"],
        "2.17.0".into()
    );
}

#[test]
fn test_decode_llmobs_body_empty_spans() {
    let body = Bytes::from(r#"[{"event_type": "span", "spans": []}]"#);
    let source = make_llmobs_source();
    let events = decode_llmobs_body(body, None, &source).unwrap();
    assert_eq!(events.len(), 0);
}

#[test]
fn test_decode_llmobs_body_invalid_json() {
    let body = Bytes::from("not json");
    let source = make_llmobs_source();
    assert!(decode_llmobs_body(body, None, &source).is_err());
}

#[test]
fn test_decode_llmobs_body_api_key() {
    let body = Bytes::from(r#"[{"event_type":"span","spans":[{"span_id":"a","trace_id":"b"}]}]"#);
    let api_key: Option<Arc<str>> = Some(Arc::from("test1234test1234test1234test1234"));
    let source = make_llmobs_source();

    let events = decode_llmobs_body(body, api_key, &source).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0]
            .metadata()
            .datadog_api_key()
            .map(|k| k.as_ref().to_owned()),
        Some("test1234test1234test1234test1234".to_owned())
    );
}
