use std::convert::TryFrom;

use vector_lib::lookup::PathPrefix;

use crate::{
    codecs::Transformer,
    event::{LogEvent, Metric, MetricKind, MetricValue, ObjectMap, Value},
    sinks::{
        elasticsearch::{
            sink::process_log, BulkAction, BulkConfig, DataStreamConfig, ElasticsearchApiVersion,
            ElasticsearchCommon, ElasticsearchConfig, ElasticsearchMode, VersionType,
        },
        util::encoding::Encoder,
    },
    template::Template,
};

// helper to unwrap template strings for tests only
fn parse_template(input: &str) -> Template {
    Template::try_from(input).unwrap()
}

#[tokio::test]
async fn sets_create_action_when_configured() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_template("{{ action }}te"),
            index: parse_template("vector"),
            version: None,
            version_type: VersionType::Internal,
        },
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );
    log.insert("action", "crea");

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_index":"vector","_type":"_doc"}}
{"action":"crea","message":"hello there","timestamp":"2020-12-01T01:02:03Z"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn encoding_with_external_versioning_without_version_set_does_not_include_version() {
    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_template("create"),
            index: parse_template("vector"),
            version: None,
            version_type: VersionType::External,
        },
        id_key: Some("my_id".into()),
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await;
    assert!(es.is_err());
}

#[tokio::test]
async fn encoding_with_external_versioning_with_version_set_includes_version() {
    use crate::config::log_schema;
    use chrono::{TimeZone, Utc};

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_template("create"),
            index: parse_template("vector"),
            version: Some(parse_template("{{ my_field }}")),
            version_type: VersionType::External,
        },
        id_key: Some("my_id".into()),
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config)
        .await
        .expect("config creation failed");

    let mut log = LogEvent::from("hello there");
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );
    log.insert("my_field", "1337");
    log.insert("my_id", "42");

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, config.id_key.as_ref(), &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_index":"vector","_type":"_doc","_id":"42","version_type":"external","version":1337}}
{"message":"hello there","my_field":"1337","timestamp":"2020-12-01T01:02:03Z"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn encoding_with_external_gte_versioning_with_version_set_includes_version() {
    use crate::config::log_schema;
    use chrono::{TimeZone, Utc};

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_template("create"),
            index: parse_template("vector"),
            version: Some(parse_template("{{ my_field }}")),
            version_type: VersionType::ExternalGte,
        },
        id_key: Some("my_id".into()),
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config)
        .await
        .expect("config creation failed");

    let mut log = LogEvent::from("hello there");
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );
    log.insert("my_field", "1337");
    log.insert("my_id", "42");

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, config.id_key.as_ref(), &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_index":"vector","_type":"_doc","_id":"42","version_type":"external_gte","version":1337}}
{"message":"hello there","my_field":"1337","timestamp":"2020-12-01T01:02:03Z"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

fn data_stream_body(
    dtype: Option<String>,
    dataset: Option<String>,
    namespace: Option<String>,
) -> ObjectMap {
    let mut ds = ObjectMap::new();

    if let Some(dtype) = dtype {
        ds.insert("type".into(), Value::from(dtype));
    }

    if let Some(dataset) = dataset {
        ds.insert("dataset".into(), Value::from(dataset));
    }

    if let Some(namespace) = namespace {
        ds.insert("namespace".into(), Value::from(namespace));
    }

    ds
}

#[tokio::test]
async fn encode_datastream_mode() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            index: parse_template("vector"),
            ..Default::default()
        },
        endpoints: vec![String::from("https://example.com")],
        mode: ElasticsearchMode::DataStream,
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );
    log.insert(
        "data_stream",
        data_stream_body(
            Some("synthetics".to_string()),
            Some("testing".to_string()),
            None,
        ),
    );

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
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
        bulk: BulkConfig {
            index: parse_template("vector"),
            ..Default::default()
        },
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
    log.insert(
        "data_stream",
        data_stream_body(
            Some("synthetics".to_string()),
            Some("testing".to_string()),
            None,
        ),
    );
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );
    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
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
        bulk: BulkConfig {
            action: parse_template("create"),
            index: parse_template("vector"),
            ..Default::default()
        },
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
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let encoded = std::str::from_utf8(&encoded).unwrap();
    let encoded_lines = encoded.split('\n').map(String::from).collect::<Vec<_>>();
    assert_eq!(encoded_lines.len(), 3); // there's an empty line at the end
    assert_eq!(
        encoded_lines.first().unwrap(),
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
        bulk: BulkConfig {
            action: parse_template("{{ action }}"),
            index: parse_template("vector"),
            ..Default::default()
        },
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

/// validates that the configuration parsing for ElasticsearchCommon succeeds when BulkConfig is
/// not explicitly set in the configuration (using defaults).
#[tokio::test]
async fn default_bulk_settings() {
    let config = ElasticsearchConfig {
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V7,
        ..Default::default()
    };
    assert!(ElasticsearchCommon::parse_single(&config).await.is_ok());
}

#[tokio::test]
async fn decode_bulk_action() {
    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_template("create"),
            index: parse_template("vector"),
            ..Default::default()
        },
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
        bulk: BulkConfig {
            index: parse_template("vector"),
            ..Default::default()
        },
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
    log.insert(
        "data_stream",
        data_stream_body(
            Some("synthetics".to_string()),
            Some("testing".to_string()),
            None,
        ),
    );
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
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
        bulk: BulkConfig {
            index: parse_template("{{ idx }}"),
            ..Default::default()
        },
        encoding: Transformer::new(None, Some(vec!["idx".into(), "timestamp".into()]), None)
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
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
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
        bulk: BulkConfig {
            index: parse_template("{{ idx }}"),
            ..Default::default()
        },
        encoding: Transformer::new(Some(vec!["foo".into()]), None, None).unwrap(),
        endpoints: vec![String::from("https://example.com")],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert("foo", "bar");
    log.insert("idx", "purple");

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"index":{"_index":"purple","_type":"_doc"}}
{"foo":"bar"}
"#;
    assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn datastream_index_name() {
    #[derive(Clone, Debug)]
    struct TestCase {
        dtype: Option<String>,
        namespace: Option<String>,
        dataset: Option<String>,
        want: String,
    }

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            index: parse_template("vector"),
            ..Default::default()
        },
        endpoints: vec![String::from("https://example.com")],
        mode: ElasticsearchMode::DataStream,
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let test_cases = [
        TestCase {
            dtype: Some("type".to_string()),
            dataset: Some("dataset".to_string()),
            namespace: Some("namespace".to_string()),
            want: "type-dataset-namespace".to_string(),
        },
        TestCase {
            dtype: Some("type".to_string()),
            dataset: Some("".to_string()),
            namespace: Some("namespace".to_string()),
            want: "type-namespace".to_string(),
        },
        TestCase {
            dtype: Some("type".to_string()),
            dataset: None,
            namespace: Some("namespace".to_string()),
            want: "type-generic-namespace".to_string(),
        },
        TestCase {
            dtype: Some("type".to_string()),
            dataset: Some("".to_string()),
            namespace: Some("".to_string()),
            want: "type".to_string(),
        },
        TestCase {
            dtype: Some("type".to_string()),
            dataset: None,
            namespace: None,
            want: "type-generic-default".to_string(),
        },
        TestCase {
            dtype: Some("".to_string()),
            dataset: Some("".to_string()),
            namespace: Some("".to_string()),
            want: "".to_string(),
        },
        TestCase {
            dtype: None,
            dataset: None,
            namespace: None,
            want: "logs-generic-default".to_string(),
        },
        TestCase {
            dtype: Some("".to_string()),
            dataset: Some("dataset".to_string()),
            namespace: Some("namespace".to_string()),
            want: "dataset-namespace".to_string(),
        },
        TestCase {
            dtype: None,
            dataset: Some("dataset".to_string()),
            namespace: Some("namespace".to_string()),
            want: "logs-dataset-namespace".to_string(),
        },
        TestCase {
            dtype: Some("".to_string()),
            dataset: Some("".to_string()),
            namespace: Some("namespace".to_string()),
            want: "namespace".to_string(),
        },
        TestCase {
            dtype: None,
            dataset: None,
            namespace: Some("namespace".to_string()),
            want: "logs-generic-namespace".to_string(),
        },
        TestCase {
            dtype: Some("".to_string()),
            dataset: Some("dataset".to_string()),
            namespace: Some("".to_string()),
            want: "dataset".to_string(),
        },
        TestCase {
            dtype: None,
            dataset: Some("dataset".to_string()),
            namespace: None,
            want: "logs-dataset-default".to_string(),
        },
    ];

    for test_case in test_cases {
        let mut log = LogEvent::from("hello there");
        log.insert(
            "data_stream",
            data_stream_body(
                test_case.dtype.clone(),
                test_case.dataset.clone(),
                test_case.namespace.clone(),
            ),
        );

        let processed_event = process_log(log, &es.mode, None, &config.encoding).unwrap();
        assert_eq!(processed_event.index, test_case.want, "{test_case:?}");
    }
}
