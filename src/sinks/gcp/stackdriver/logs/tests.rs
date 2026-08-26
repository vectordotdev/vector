//! Unit tests for the `gcp_stackdriver_logs` sink.

use std::collections::HashMap;

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{future::ready, stream};
use http::Uri;
use indoc::indoc;
use vector_lib::lookup::lookup_v2::ConfigValuePath;
use vrl::{event_path, value};

use super::{
    config::{StackdriverConfig, default_endpoint},
    encoder::{
        StackdriverLabelConfig as EncoderLabelConfig, StackdriverLogsEncoder,
        StackdriverResource as EncoderResource,
    },
};
use crate::{
    config::{GenerateConfig, SinkConfig, SinkContext},
    event::{LogEvent, Value},
    gcp::GcpAuthenticator,
    sinks::{
        gcp::stackdriver::logs::{
            config::StackdriverLogName, encoder::remap_severity,
            service::StackdriverLogsServiceRequestBuilder,
        },
        prelude::*,
        util::{
            HttpEndpoint,
            encoding::Encoder as _,
            http::{HttpRequest, HttpServiceRequestBuilder},
        },
    },
    template::{ConfinementConfig, Template, UnconfinedTemplate},
    test_util::{
        components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        http::{always_200_response, spawn_blackhole_http_server},
    },
};

// Tests exercise the encoder, not confinement; build a checkerless confined template.
fn confined(s: &str) -> ConfinedTemplate {
    Template::try_from(s)
        .unwrap()
        .confine(
            &ConfinementConfig::unconfined(),
            StackdriverConfig::NAME,
            "template",
        )
        .unwrap()
}

fn unconfined(s: &str) -> UnconfinedTemplate {
    UnconfinedTemplate::try_from(s).unwrap()
}

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<StackdriverConfig>();
}

#[tokio::test]
async fn component_spec_compliance() {
    let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

    let mut config: StackdriverConfig =
        serde_json::from_value(StackdriverConfig::generate_config())
            .expect("config should be valid");

    // If we don't override the credentials path/API key, it tries to directly call out to the Google Instance
    // Metadata API, which we clearly don't have in unit tests. :)
    config.auth.credentials_path = None;
    config.auth.api_key = Some("fake".to_string().into());
    config.endpoint = HttpEndpoint::parse(&mock_endpoint.to_string()).unwrap();

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
        confined("{{ log_id }}"),
        StackdriverLogName::Project("project".to_owned()),
        EncoderLabelConfig {
            labels_key: None,
            labels: HashMap::from([(
                "config_user_label_1".to_owned(),
                unconfined("config_user_value_1"),
            )]),
        },
        EncoderResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([
                ("namespace".to_owned(), unconfined("office")),
                ("node_id".to_owned(), unconfined("{{ node_id }}")),
            ]),
        },
        Some(ConfigValuePath::try_from("anumber".to_owned()).unwrap()),
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
        confined("testlogs"),
        StackdriverLogName::Project("project".to_owned()),
        EncoderLabelConfig {
            labels_key: None,
            labels: HashMap::from([("config_user_label_1".to_owned(), unconfined("value_1"))]),
        },
        EncoderResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([("namespace".to_owned(), unconfined("office"))]),
        },
        Some(ConfigValuePath::try_from("anumber".to_owned()).unwrap()),
    );

    let mut log = LogEvent::default();
    log.insert(event_path!("message"), Value::Bytes("hello world".into()));
    log.insert(event_path!("anumber"), Value::Bytes("100".into()));
    log.insert(
        event_path!("timestamp"),
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
    let uri: Uri = default_endpoint().into_uri();

    let transformer = Transformer::default();
    let encoder = StackdriverLogsEncoder::new(
        transformer,
        confined("testlogs"),
        StackdriverLogName::Project("project".to_owned()),
        EncoderLabelConfig {
            labels_key: None,
            labels: HashMap::from([("config_user_label_1".to_owned(), unconfined("value_1"))]),
        },
        EncoderResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([("namespace".to_owned(), unconfined("office"))]),
        },
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
    let config: StackdriverConfig = serde_yaml::from_str(indoc! {r#"
            project_id: project
            log_id: testlogs
            resource:
              type: generic_node
              namespace: office
        "#})
    .unwrap();
    if config.build(SinkContext::default()).await.is_ok() {
        panic!("config.build failed to error");
    }
}

#[test]
fn fails_invalid_log_names() {
    serde_yaml::from_str::<StackdriverConfig>(indoc! {r#"
            log_id: testlogs
            resource:
              type: generic_node
              namespace: office
        "#})
    .expect_err("Config parsing failed to error with missing ids");

    serde_yaml::from_str::<StackdriverConfig>(indoc! {r#"
            project_id: project
            folder_id: folder
            log_id: testlogs
            resource:
              type: generic_node
              namespace: office
        "#})
    .expect_err("Config parsing failed to error with extraneous ids");
}
