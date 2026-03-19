//! Unit tests for the `gcp_stackdriver_logs` sink.

use std::collections::HashMap;

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{future::ready, stream};
use http::Uri;
use indoc::indoc;
use serde::Deserialize;
use vector_lib::lookup::lookup_v2::ConfigValuePath;
use vrl::{event_path, value};

use super::{
    config::{StackdriverConfig, StackdriverResource, default_endpoint},
    encoder::StackdriverLogsEncoder,
};
use crate::{
    config::{GenerateConfig, SinkConfig, SinkContext},
    event::{LogEvent, Value},
    gcp::GcpAuthenticator,
    sinks::{
        gcp::stackdriver::logs::{
            config::{StackdriverLabelConfig, StackdriverLogName},
            encoder::remap_severity,
            service::StackdriverLogsServiceRequestBuilder,
        },
        prelude::*,
        util::{
            encoding::Encoder as _,
            http::{HttpRequest, HttpServiceRequestBuilder},
        },
    },
    test_util::{
        components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        http::{always_200_response, spawn_blackhole_http_server},
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<StackdriverConfig>();
}

#[tokio::test]
async fn component_spec_compliance() {
    let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

    let config = StackdriverConfig::generate_config().to_string();
    let mut config = StackdriverConfig::deserialize(
        toml::de::ValueDeserializer::parse(&config).expect("toml should deserialize"),
    )
    .expect("config should be valid");

    // If we don't override the credentials path/API key, it tries to directly call out to the Google Instance
    // Metadata API, which we clearly don't have in unit tests. :)
    config.auth.credentials_path = None;
    config.auth.api_key = Some("fake".to_string().into());
    config.endpoint = mock_endpoint.to_string();

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();

    let event = Event::Log(LogEvent::from("simple message"));
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;
}

#[test]
fn encode_valid() {
    let mut transformer = Transformer::default();
    transformer
        .set_except_fields(Some(vec![
            "anumber".into(),
            "node_id".into(),
            "log_id".into(),
        ]))
        .unwrap();

    let encoder = StackdriverLogsEncoder::new(
        transformer,
        Template::try_from("{{ log_id }}").unwrap(),
        StackdriverLogName::Project("project".to_owned()),
        StackdriverLabelConfig {
            labels_key: None,
            labels: HashMap::from([(
                "config_user_label_1".to_owned(),
                Template::try_from("config_user_value_1").unwrap(),
            )]),
        },
        StackdriverResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([
                (
                    "namespace".to_owned(),
                    Template::try_from("office").unwrap(),
                ),
                (
                    "node_id".to_owned(),
                    Template::try_from("{{ node_id }}").unwrap(),
                ),
            ]),
        },
        Some(ConfigValuePath::try_from("anumber".to_owned()).unwrap()),
        None,
    );

    let mut log = [
        ("message", "hello world"),
        ("anumber", "100"),
        ("node_id", "10.10.10.1"),
        ("log_id", "testlogs"),
    ]
    .iter()
    .copied()
    .collect::<LogEvent>();
    log.insert(
        event_path!("logging.googleapis.com/labels"),
        value!({user_label_1: "user_value_1"}),
    );

    let json = encoder.encode_event(Event::from(log)).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "logName":"projects/project/logs/testlogs",
            "jsonPayload":{"message":"hello world"},
            "severity":100,
            "labels":{
                "config_user_label_1":"config_user_value_1",
                "user_label_1":"user_value_1"
            },
            "resource":{
                "type":"generic_node",
                "labels":{"namespace":"office","node_id":"10.10.10.1"}
            }
        })
    );
}

#[test]
fn encode_inserts_timestamp() {
    let transformer = Transformer::default();

    let encoder = StackdriverLogsEncoder::new(
        transformer,
        Template::try_from("testlogs").unwrap(),
        StackdriverLogName::Project("project".to_owned()),
        StackdriverLabelConfig {
            labels_key: None,
            labels: HashMap::from([(
                "config_user_label_1".to_owned(),
                Template::try_from("value_1").unwrap(),
            )]),
        },
        StackdriverResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([(
                "namespace".to_owned(),
                Template::try_from("office").unwrap(),
            )]),
        },
        Some(ConfigValuePath::try_from("anumber".to_owned()).unwrap()),
        None,
    );

    let mut log = LogEvent::default();
    log.insert("message", Value::Bytes("hello world".into()));
    log.insert("anumber", Value::Bytes("100".into()));
    log.insert(
        "timestamp",
        Value::Timestamp(
            Utc.with_ymd_and_hms(2020, 1, 1, 12, 30, 0)
                .single()
                .expect("invalid timestamp"),
        ),
    );

    let json = encoder.encode_event(Event::from(log)).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "logName":"projects/project/logs/testlogs",
            "jsonPayload":{"message":"hello world","timestamp":"2020-01-01T12:30:00Z"},
            "severity":100,
            "labels":{"config_user_label_1":"value_1"},
            "resource":{
                "type":"generic_node",
                "labels":{"namespace":"office"}},
            "timestamp":"2020-01-01T12:30:00Z"
        })
    );
}

#[test]
fn encode_with_insert_id_key() {
    let transformer = Transformer::default();

    let encoder = StackdriverLogsEncoder::new(
        transformer,
        Template::try_from("testlogs").unwrap(),
        StackdriverLogName::Project("project".to_owned()),
        StackdriverLabelConfig {
            labels_key: None,
            labels: HashMap::new(),
        },
        StackdriverResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([(
                "namespace".to_owned(),
                Template::try_from("office").unwrap(),
            )]),
        },
        None,
        Some(ConfigValuePath::try_from("insert_id".to_owned()).unwrap()),
    );

    let log = [
        ("message", "hello world"),
        ("insert_id", "topic.access.log-0-12345"),
    ]
    .iter()
    .copied()
    .collect::<LogEvent>();

    let json = encoder.encode_event(Event::from(log)).unwrap();

    assert_eq!(
        json,
        serde_json::json!({
            "logName": "projects/project/logs/testlogs",
            "jsonPayload": {"message": "hello world"},
            "severity": 0,
            "labels": {},
            "resource": {
                "type": "generic_node",
                "labels": {"namespace": "office"}
            },
            "insertId": "topic.access.log-0-12345"
        })
    );

    // Verify insert_id field was removed from jsonPayload
    assert!(
        !json["jsonPayload"]
            .as_object()
            .unwrap()
            .contains_key("insert_id")
    );
}

#[test]
fn encode_without_insert_id_key() {
    let transformer = Transformer::default();

    let encoder = StackdriverLogsEncoder::new(
        transformer,
        Template::try_from("testlogs").unwrap(),
        StackdriverLogName::Project("project".to_owned()),
        StackdriverLabelConfig {
            labels_key: None,
            labels: HashMap::new(),
        },
        StackdriverResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([(
                "namespace".to_owned(),
                Template::try_from("office").unwrap(),
            )]),
        },
        None,
        None,
    );

    let log = [
        ("message", "hello world"),
        ("insert_id", "should-remain-in-payload"),
    ]
    .iter()
    .copied()
    .collect::<LogEvent>();

    let json = encoder.encode_event(Event::from(log)).unwrap();

    // insertId should NOT be in the LogEntry json
    assert!(!json.as_object().unwrap().contains_key("insertId"));

    // insert_id should remain in jsonPayload since we didn't configure insert_id_key
    assert_eq!(
        json["jsonPayload"]["insert_id"],
        serde_json::json!("should-remain-in-payload")
    );
}

#[test]
fn encode_insert_id_type_coercion() {
    let transformer = Transformer::default();

    let encoder = StackdriverLogsEncoder::new(
        transformer,
        Template::try_from("testlogs").unwrap(),
        StackdriverLogName::Project("project".to_owned()),
        StackdriverLabelConfig {
            labels_key: None,
            labels: HashMap::new(),
        },
        StackdriverResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([(
                "namespace".to_owned(),
                Template::try_from("office").unwrap(),
            )]),
        },
        None,
        Some(ConfigValuePath::try_from("insert_id".to_owned()).unwrap()),
    );

    let mut log = LogEvent::default();
    log.insert("message", Value::Bytes("hello".into()));
    log.insert("insert_id", Value::Integer(12345));

    let json = encoder.encode_event(Event::from(log)).unwrap();

    assert_eq!(json["insertId"], serde_json::json!("12345"));
}

#[test]
fn encode_insert_id_field_missing() {
    let transformer = Transformer::default();

    let encoder = StackdriverLogsEncoder::new(
        transformer,
        Template::try_from("testlogs").unwrap(),
        StackdriverLogName::Project("project".to_owned()),
        StackdriverLabelConfig {
            labels_key: None,
            labels: HashMap::new(),
        },
        StackdriverResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([(
                "namespace".to_owned(),
                Template::try_from("office").unwrap(),
            )]),
        },
        None,
        Some(ConfigValuePath::try_from("insert_id".to_owned()).unwrap()),
    );

    let log = [("message", "hello world")]
        .iter()
        .copied()
        .collect::<LogEvent>();

    let json = encoder.encode_event(Event::from(log)).unwrap();

    // insertId should NOT be in LogEntry if key was supplied but field was missing
    assert!(!json.as_object().unwrap().contains_key("insertId"));
}

#[test]
fn severity_remaps_strings() {
    for &(s, n) in &[
        ("EMERGENCY", 800), // Handles full upper case
        ("EMERG", 800),     // Handles abbreviations
        ("FATAL", 800),     // Handles highest alternate
        ("alert", 700),     // Handles lower case
        ("CrIt1c", 600),    // Handles mixed case and suffixes
        ("err404", 500),    // Handles lower case and suffixes
        ("warnings", 400),
        ("notice", 300),
        ("info", 200),
        ("DEBUG2", 100), // Handles upper case and suffixes
        ("trace", 100),  // Handles lowest alternate
        ("nothing", 0),  // Maps unknown terms to DEFAULT
        ("123", 100),    // Handles numbers in strings
        ("-100", 0),     // Maps negatives to DEFAULT
    ] {
        assert_eq!(
            remap_severity(s.into()),
            Value::Integer(n),
            "remap_severity({s:?}) != {n}"
        );
    }
}

#[tokio::test]
async fn correct_request() {
    let uri: Uri = default_endpoint().parse().unwrap();

    let transformer = Transformer::default();
    let encoder = StackdriverLogsEncoder::new(
        transformer,
        Template::try_from("testlogs").unwrap(),
        StackdriverLogName::Project("project".to_owned()),
        StackdriverLabelConfig {
            labels_key: None,
            labels: HashMap::from([(
                "config_user_label_1".to_owned(),
                Template::try_from("value_1").unwrap(),
            )]),
        },
        StackdriverResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([(
                "namespace".to_owned(),
                Template::try_from("office").unwrap(),
            )]),
        },
        None,
        None,
    );

    let log1 = [("message", "hello")].iter().copied().collect::<LogEvent>();
    let log2 = [("message", "world")].iter().copied().collect::<LogEvent>();

    let events = vec![Event::from(log1), Event::from(log2)];

    let mut writer = Vec::new();
    let (_, _) = encoder.encode_input(events, &mut writer).unwrap();

    let body = Bytes::copy_from_slice(&writer);

    let stackdriver_logs_service_request_builder = StackdriverLogsServiceRequestBuilder {
        uri: uri.clone(),
        auth: GcpAuthenticator::None,
    };

    let http_request = HttpRequest::new(
        body,
        EventFinalizers::default(),
        RequestMetadata::default(),
        (),
    );

    let request = stackdriver_logs_service_request_builder
        .build(http_request)
        .unwrap();

    let (parts, body) = request.into_parts();
    let json: serde_json::Value = serde_json::from_slice(&body[..]).unwrap();

    assert_eq!(
        &parts.uri.to_string(),
        "https://logging.googleapis.com/v2/entries:write"
    );
    assert_eq!(
        json,
        serde_json::json!({
            "entries": [
                {
                    "logName": "projects/project/logs/testlogs",
                    "severity": 0,
                    "jsonPayload": {
                        "message": "hello"
                    },
                    "labels": {
                        "config_user_label_1": "value_1"
                    },
                    "resource": {
                        "type": "generic_node",
                        "labels": {
                            "namespace": "office"
                        }
                    }
                },
                {
                    "logName": "projects/project/logs/testlogs",
                    "severity": 0,
                    "jsonPayload": {
                        "message": "world"
                    },
                    "labels": {
                        "config_user_label_1": "value_1"
                    },
                    "resource": {
                        "type": "generic_node",
                        "labels": {
                            "namespace": "office"
                        }
                    }
                }
            ]
        })
    );
}

#[tokio::test]
async fn fails_missing_creds() {
    let config: StackdriverConfig = toml::from_str(indoc! {r#"
            project_id = "project"
            log_id = "testlogs"
            resource.type = "generic_node"
            resource.namespace = "office"
            credentials_path = {missing_credentials_path:?}
        "#}))
    .unwrap();

    let error = config
        .build(SinkContext::default())
        .await
        .expect_err("config.build failed to error");
    assert_downcast_matches!(error, GcpError, GcpError::InvalidCredentials { .. });
}

#[test]
fn fails_invalid_log_names() {
    toml::from_str::<StackdriverConfig>(indoc! {r#"
            log_id = "testlogs"
            resource.type = "generic_node"
            resource.namespace = "office"
        "#})
    .expect_err("Config parsing failed to error with missing ids");

    toml::from_str::<StackdriverConfig>(indoc! {r#"
            project_id = "project"
            folder_id = "folder"
            log_id = "testlogs"
            resource.type = "generic_node"
            resource.namespace = "office"
        "#})
    .expect_err("Config parsing failed to error with extraneous ids");
}
