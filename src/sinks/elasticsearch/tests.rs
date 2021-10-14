use crate::sinks::elasticsearch::{ElasticSearchConfig, ElasticSearchCommon, ElasticSearchMode, DataStreamConfig, ElasticSearchAuth};
use crate::event::{LogEvent, Event, Value, Metric, MetricKind, MetricValue};
use crate::sinks::elasticsearch::encoder::{ElasticSearchEncoder, ProcessedEvent};
use crate::sinks::util::encoding::{Encoder, EncodingConfigWithDefault, EncodingConfigFixed};
use crate::sinks::elasticsearch::sink::process_log;
use std::collections::BTreeMap;
use crate::template::Template;
use std::convert::TryFrom;
use http::{Response, StatusCode, Uri};
use bytes::Bytes;
use crate::sinks::elasticsearch::retry::ElasticSearchRetryLogic;
use crate::sinks::util::retries::{RetryLogic, RetryAction};
use super::BulkAction;
use crate::rusoto::AwsAuthentication;
use crate::sinks::util::BatchConfig;
use crate::sinks::elasticsearch::service::ElasticSearchResponse;

#[test]
fn sets_create_action_when_configured() {
    use crate::config::log_schema;
    use chrono::{TimeZone, Utc};

    let config = ElasticSearchConfig {
        bulk_action: Some(String::from("{{ action }}te")),
        index: Some(String::from("vector")),
        endpoint: String::from("https://example.com"),
        ..Default::default()
    };
    let es = ElasticSearchCommon::parse_config(&config).unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(
        log_schema().timestamp_key(),
        Utc.ymd(2020, 12, 1).and_hms(1, 2, 3),
    );
    log.insert("action", "crea");

    let mut encoded = vec![];
    let encoded_size = es.encoding.encode_input(vec![
        process_log(log, &es.mode, &None, es.doc_type).unwrap()
    ], &mut encoded).unwrap();

    let expected = r#"{"create":{"_index":"vector","_type":"_doc"}}
{"action":"crea","message":"hello there","timestamp":"2020-12-01T01:02:03Z"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

fn data_stream_body() -> BTreeMap<String, Value> {
    let mut ds = BTreeMap::<String, Value>::new();
    ds.insert("type".into(), Value::from("synthetics"));
    ds.insert("dataset".into(), Value::from("testing"));
    ds
}

#[test]
fn encode_datastream_mode() {
    use crate::config::log_schema;
    use chrono::{TimeZone, Utc};

    let config = ElasticSearchConfig {
        index: Some(String::from("vector")),
        endpoint: String::from("https://example.com"),
        mode: ElasticSearchMode::DataStream,
        ..Default::default()
    };
    let es = ElasticSearchCommon::parse_config(&config).unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(
        log_schema().timestamp_key(),
        Utc.ymd(2020, 12, 1).and_hms(1, 2, 3),
    );
    log.insert("data_stream", data_stream_body());

    let mut encoded = vec![];
    let encoded_size = es.encoding.encode_input(vec![
        process_log(log, &es.mode, &None, es.doc_type).unwrap()
    ], &mut encoded).unwrap();

    let expected = r#"{"create":{"_index":"synthetics-testing-default","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","namespace":"default","type":"synthetics"},"message":"hello there"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

#[test]
fn encode_datastream_mode_no_routing() {
    use crate::config::log_schema;
    use chrono::{TimeZone, Utc};

    let config = ElasticSearchConfig {
        index: Some(String::from("vector")),
        endpoint: String::from("https://example.com"),
        mode: ElasticSearchMode::DataStream,
        data_stream: Some(DataStreamConfig {
            auto_routing: false,
            namespace: Template::try_from("something").unwrap(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let es = ElasticSearchCommon::parse_config(&config).unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert("data_stream", data_stream_body());
    log.insert(
        log_schema().timestamp_key(),
        Utc.ymd(2020, 12, 1).and_hms(1, 2, 3),
    );
    let mut encoded = vec![];
    let encoded_size = es.encoding.encode_input(vec![
        process_log(log, &es.mode, &None, es.doc_type).unwrap()
    ], &mut encoded).unwrap();

    let expected = r#"{"create":{"_index":"logs-generic-something","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","namespace":"something","type":"synthetics"},"message":"hello there"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

#[test]
fn handle_metrics() {
    let config = ElasticSearchConfig {
        bulk_action: Some(String::from("create")),
        index: Some(String::from("vector")),
        endpoint: String::from("https://example.com"),
        ..Default::default()
    };
    let es = ElasticSearchCommon::parse_config(&config).unwrap();

    let metric = Metric::new(
        "cpu",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 42.0 },
    );
    let log = es.metric_to_log.transform_one(metric).unwrap();

    let mut encoded = vec![];
    let encoded_size = es.encoding.encode_input(vec![
        process_log(log, &es.mode, &None, es.doc_type).unwrap()
    ], &mut encoded).unwrap();

    let encoded = std::str::from_utf8(&encoded).unwrap();
    let encoded_lines = encoded.split('\n').map(String::from).collect::<Vec<_>>();
    assert_eq!(encoded_lines.len(), 3); // there's an empty line at the end
    assert_eq!(
        encoded_lines.get(0).unwrap(),
        r#"{"create":{"_index":"vector","_type":"_doc"}}"#
    );
    assert!(encoded_lines
        .get(1)
        .unwrap()
        .starts_with(r#"{"gauge":{"value":42.0},"kind":"absolute","name":"cpu","timestamp""#));
}

#[test]
fn decode_bulk_action_error() {
    let config = ElasticSearchConfig {
        bulk_action: Some(String::from("{{ action }}")),
        index: Some(String::from("vector")),
        endpoint: String::from("https://example.com"),
        ..Default::default()
    };
    let es = ElasticSearchCommon::parse_config(&config).unwrap();

    let mut log = LogEvent::from("hello world");
    log.insert("foo", "bar");
    log.insert("idx", "purple");
    let action = es.mode.bulk_action(&log);
    assert!(action.is_none());
}

#[test]
fn decode_bulk_action() {
    let config = ElasticSearchConfig {
        bulk_action: Some(String::from("create")),
        index: Some(String::from("vector")),
        endpoint: String::from("https://example.com"),
        ..Default::default()
    };
    let es = ElasticSearchCommon::parse_config(&config).unwrap();

    let log = LogEvent::from("hello there");
    let action = es.mode.bulk_action(&log).unwrap();
    assert!(matches!(action, BulkAction::Create));
}

#[test]
fn encode_datastream_mode_no_sync() {
    use crate::config::log_schema;
    use chrono::{TimeZone, Utc};

    let config = ElasticSearchConfig {
        index: Some(String::from("vector")),
        endpoint: String::from("https://example.com"),
        mode: ElasticSearchMode::DataStream,
        data_stream: Some(DataStreamConfig {
            namespace: Template::try_from("something").unwrap(),
            sync_fields: false,
            ..Default::default()
        }),
        ..Default::default()
    };

    let es = ElasticSearchCommon::parse_config(&config).unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert("data_stream", data_stream_body());
    log.insert(
        log_schema().timestamp_key(),
        Utc.ymd(2020, 12, 1).and_hms(1, 2, 3),
    );

    let mut encoded = vec![];
    let encoded_size = es.encoding.encode_input(vec![
        process_log(log, &es.mode, &None, es.doc_type).unwrap()
    ], &mut encoded).unwrap();

    let expected = r#"{"create":{"_index":"synthetics-testing-something","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","type":"synthetics"},"message":"hello there"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

#[test]
fn allows_using_excepted_fields() {
    let config = ElasticSearchConfig {
        index: Some(String::from("{{ idx }}")),
        encoding: EncodingConfigFixed {
            except_fields: Some(vec!["idx".to_string(), "timestamp".to_string()]),
            ..Default::default()
        },
        endpoint: String::from("https://example.com"),
        ..Default::default()
    };
    let es = ElasticSearchCommon::parse_config(&config).unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert("foo", "bar");
    log.insert("idx", "purple");

    let mut encoded = vec![];
    let encoded_size = es.encoding.encode_input(vec![
        process_log(log, &es.mode, &None, es.doc_type).unwrap()
    ], &mut encoded).unwrap();

    let expected = r#"{"index":{"_index":"purple","_type":"_doc"}}
{"foo":"bar","message":"hello there"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

#[test]
fn validate_host_header_on_aws_requests() {
    let config = ElasticSearchConfig {
        auth: Some(ElasticSearchAuth::Aws(AwsAuthentication::Default {})),
        endpoint: "http://abc-123.us-east-1.es.amazonaws.com".into(),
        batch: BatchConfig {
            max_events: Some(1),
            ..Default::default()
        },
        ..Default::default()
    };

    let common = ElasticSearchCommon::parse_config(&config).expect("Config error");

    let signed_request = common.signed_request(
        "POST",
        &"http://abc-123.us-east-1.es.amazonaws.com"
            .parse::<Uri>()
            .unwrap(),
        true,
    );

    assert_eq!(
        signed_request.hostname(),
        "abc-123.us-east-1.es.amazonaws.com".to_string()
    );
}



