use std::{collections::BTreeMap, convert::TryFrom};

use crate::{
    codecs::Transformer,
    event::{LogEvent, Metric, MetricKind, MetricValue, Value},
    sinks::{
        elasticsearch::{
            sink::process_log, BulkAction, BulkConfig, DataStreamConfig, ElasticsearchApiVersion,
            ElasticsearchCommon, ElasticsearchConfig, ElasticsearchMode,
        },
        util::encoding::Encoder,
    },
    template::Template,
};
use lookup::owned_value_path;

// helper to unwrap template strings for tests only
fn parse_template(input: &str) -> Template {
    Template::try_from(input).unwrap()
}

#[tokio::test]
async fn sets_create_action_when_configured() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: Some(BulkConfig {
            action: parse_template("{{ action }}te"),
            index: parse_template("vector"),
        }),
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(
        log_schema().timestamp_key(),
        Utc.ymd(2020, 12, 1)
            .and_hms_opt(1, 2, 3)
            .expect("invalid timestamp"),
    );
    log.insert("action", "crea");

    let mut encoded = vec![];
    let encoded_size = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, &None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

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

#[tokio::test]
async fn encode_datastream_mode() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: Some(BulkConfig {
            index: parse_template("vector"),
            ..Default::default()
        }),
        endpoints: vec![String::from("https://example.com")],
        mode: ElasticsearchMode::DataStream,
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(
        log_schema().timestamp_key(),
        Utc.ymd(2020, 12, 1)
            .and_hms_opt(1, 2, 3)
            .expect("invalid timestamp"),
    );
    log.insert("data_stream", data_stream_body());

    let mut encoded = vec![];
    let encoded_size = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, &None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_index":"synthetics-testing-default","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","namespace":"default","type":"synthetics"},"message":"hello there"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn encode_datastream_mode_no_routing() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: Some(BulkConfig {
            index: parse_template("vector"),
            ..Default::default()
        }),
        endpoints: vec![String::from("https://example.com")],
        mode: ElasticsearchMode::DataStream,
        data_stream: Some(DataStreamConfig {
            auto_routing: false,
            namespace: Template::try_from("something").unwrap(),
            ..Default::default()
        }),
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert("data_stream", data_stream_body());
    log.insert(
        log_schema().timestamp_key(),
        Utc.ymd(2020, 12, 1)
            .and_hms_opt(1, 2, 3)
            .expect("invalid timestamp"),
    );
    let mut encoded = vec![];
    let encoded_size = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, &None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_index":"logs-generic-something","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","namespace":"something","type":"synthetics"},"message":"hello there"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn handle_metrics() {
    let config = ElasticsearchConfig {
        bulk: Some(BulkConfig {
            action: parse_template("create"),
            index: parse_template("vector"),
        }),
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let metric = Metric::new(
        "cpu",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 42.0 },
    );
    let log = es.metric_to_log.transform_one(metric).unwrap();

    let mut encoded = vec![];
    es.request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, &None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

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

#[tokio::test]
async fn decode_bulk_action_error() {
    let config = ElasticsearchConfig {
        bulk: Some(BulkConfig {
            action: parse_template("{{ action }}"),
            index: parse_template("vector"),
        }),
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V7,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello world");
    log.insert("foo", "bar");
    log.insert("idx", "purple");
    let action = es.mode.bulk_action(&log);
    assert!(action.is_none());
}

#[tokio::test]
async fn decode_bulk_action() {
    let config = ElasticsearchConfig {
        bulk: Some(BulkConfig {
            action: parse_template("create"),
            index: parse_template("vector"),
        }),
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V7,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let log = LogEvent::from("hello there");
    let action = es.mode.bulk_action(&log).unwrap();
    assert!(matches!(action, BulkAction::Create));
}

#[tokio::test]
async fn encode_datastream_mode_no_sync() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: Some(BulkConfig {
            index: parse_template("vector"),
            ..Default::default()
        }),
        endpoints: vec![String::from("https://example.com")],
        mode: ElasticsearchMode::DataStream,
        data_stream: Some(DataStreamConfig {
            namespace: Template::try_from("something").unwrap(),
            sync_fields: false,
            ..Default::default()
        }),
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };

    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert("data_stream", data_stream_body());
    log.insert(
        log_schema().timestamp_key(),
        Utc.ymd(2020, 12, 1)
            .and_hms_opt(1, 2, 3)
            .expect("invalid timestamp"),
    );

    let mut encoded = vec![];
    let encoded_size = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, &None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_index":"synthetics-testing-something","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","type":"synthetics"},"message":"hello there"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn allows_using_except_fields() {
    let config = ElasticsearchConfig {
        bulk: Some(BulkConfig {
            index: parse_template("{{ idx }}"),
            ..Default::default()
        }),
        encoding: Transformer::new(
            None,
            Some(vec!["idx".to_string(), "timestamp".to_string()]),
            None,
        )
        .unwrap(),
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert("foo", "bar");
    log.insert("idx", "purple");

    let mut encoded = vec![];
    let encoded_size = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, &None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"index":{"_index":"purple","_type":"_doc"}}
{"foo":"bar","message":"hello there"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn allows_using_only_fields() {
    let config = ElasticsearchConfig {
        bulk: Some(BulkConfig {
            index: parse_template("{{ idx }}"),
            ..Default::default()
        }),
        encoding: Transformer::new(Some(vec![owned_value_path!("foo")]), None, None).unwrap(),
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert("foo", "bar");
    log.insert("idx", "purple");

    let mut encoded = vec![];
    let encoded_size = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, &None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"index":{"_index":"purple","_type":"_doc"}}
{"foo":"bar"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}
