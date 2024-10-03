use std::sync::Arc;

use chrono::{TimeZone, Utc};
use futures::Stream;
use futures_util::StreamExt;
use similar_asserts::assert_eq;
use tonic::Request;
use vector_lib::config::LogNamespace;
use vector_lib::lookup::path;
use vector_lib::opentelemetry::proto::{
    collector::logs::v1::{logs_service_client::LogsServiceClient, ExportLogsServiceRequest},
    common::v1::{any_value, AnyValue, KeyValue, InstrumentationScope},
    logs::v1::{LogRecord, ResourceLogs, ScopeLogs},
    resource::v1::Resource as OtelResource,
};
use vrl::value;

use crate::config::OutputId;
use crate::{
    config::{SourceConfig, SourceContext},
    event::{into_event_stream, Event, EventStatus, LogEvent, ObjectMap, Value},
    sources::opentelemetry::{GrpcConfig, HttpConfig, OpentelemetryConfig, LOGS},
    test_util::{
        self,
        components::{assert_source_compliance, SOURCE_TAGS},
        next_addr,
    },
    SourceSender,
};

#[test]
fn generate_config() {
    test_util::test_generate_config::<OpentelemetryConfig>();
}

#[tokio::test]
async fn receive_grpc_logs_vector_namespace() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let grpc_addr = next_addr();
        let http_addr = next_addr();

        let source = OpentelemetryConfig {
            grpc: GrpcConfig {
                address: grpc_addr,
                tls: Default::default(),
            },
            http: HttpConfig {
                address: http_addr,
                tls: Default::default(),
                keepalive: Default::default(),
            },
            acknowledgements: Default::default(),
            log_namespace: Some(true),
        };
        let schema_definitions = source
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        let (sender, logs_output, _) = new_source(EventStatus::Delivered, LOGS.to_string());
        let server = source
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap();
        tokio::spawn(server);
        test_util::wait_for_tcp(grpc_addr).await;

        // send request via grpc client
        let mut client = LogsServiceClient::connect(format!("http://{}", grpc_addr))
            .await
            .unwrap();
        let req = Request::new(ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(OtelResource {
                    attributes: vec![KeyValue {
                        key: "res_key".into(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("res_val".into())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                scope_logs: vec![ScopeLogs {
                    scope: Some(InstrumentationScope {
                        name: "some.scope.name".into(),
                        version: "1.2.3".into(),
                        attributes: vec![KeyValue {
                            key: "scope_attr".into(),
                            value: Some(AnyValue {
                                value: Some(any_value::Value::StringValue("scope_val".into())),
                            }),
                        }],
                        dropped_attributes_count: 7,
                    }),
                    log_records: vec![LogRecord {
                        time_unix_nano: 1,
                        observed_time_unix_nano: 2,
                        severity_number: 9,
                        severity_text: "info".into(),
                        body: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("log body".into())),
                        }),
                        attributes: vec![KeyValue {
                            key: "attr_key".into(),
                            value: Some(AnyValue {
                                value: Some(any_value::Value::StringValue("attr_val".into())),
                            }),
                        }],
                        dropped_attributes_count: 3,
                        flags: 4,
                        // opentelemetry sdk will hex::decode the given trace_id and span_id
                        trace_id: str_into_hex_bytes("4ac52aadf321c2e531db005df08792f5"),
                        span_id: str_into_hex_bytes("0b9e4bda2a55530d"),
                    }],
                    schema_url: "v1".into(),
                }],
                schema_url: "v1".into(),
            }],
        });
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(logs_output).await;
        // we just send one, so only one output
        assert_eq!(output.len(), 1);
        let event = output.pop().unwrap();
        schema_definitions.unwrap().assert_valid_for_event(&event);

        assert_eq!(event.as_log().get(".").unwrap(), &value!("log body"));

        let meta = event.as_log().metadata().value();
        assert_eq!(
            meta.get(path!("vector", "source_type")).unwrap(),
            &value!(OpentelemetryConfig::NAME)
        );
        assert!(meta
            .get(path!("vector", "ingest_timestamp"))
            .unwrap()
            .is_timestamp());
        assert_eq!(
            meta.get(path!("opentelemetry", "resources")).unwrap(),
            &value!({res_key: "res_val"})
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "attributes")).unwrap(),
            &value!({attr_key: "attr_val"})
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "scope", "name")).unwrap(),
            &value!("some.scope.name")
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "scope", "version")).unwrap(),
            &value!("1.2.3")
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "scope", "attributes")).unwrap(),
            &value!({scope_attr: "scope_val"})
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "scope", "dropped_attributes_count")).unwrap(),
            &value!(7)
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "trace_id")).unwrap(),
            &value!("4ac52aadf321c2e531db005df08792f5")
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "span_id")).unwrap(),
            &value!("0b9e4bda2a55530d")
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "severity_text")).unwrap(),
            &value!("info")
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "severity_number")).unwrap(),
            &value!(9)
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "flags")).unwrap(),
            &value!(4)
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "observed_timestamp"))
                .unwrap(),
            &value!(Utc.timestamp_nanos(2))
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "timestamp")).unwrap(),
            &value!(Utc.timestamp_nanos(1))
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "dropped_attributes_count"))
                .unwrap(),
            &value!(3)
        );
    })
    .await;
}

#[tokio::test]
async fn receive_grpc_logs_legacy_namespace() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let grpc_addr = next_addr();
        let http_addr = next_addr();

        let source = OpentelemetryConfig {
            grpc: GrpcConfig {
                address: grpc_addr,
                tls: Default::default(),
            },
            http: HttpConfig {
                address: http_addr,
                tls: Default::default(),
                keepalive: Default::default(),
            },
            acknowledgements: Default::default(),
            log_namespace: Default::default(),
        };
        let schema_definitions = source
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        let (sender, logs_output, _) = new_source(EventStatus::Delivered, LOGS.to_string());
        let server = source
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap();
        tokio::spawn(server);
        test_util::wait_for_tcp(grpc_addr).await;

        // send request via grpc client
        let mut client = LogsServiceClient::connect(format!("http://{}", grpc_addr))
            .await
            .unwrap();
        let req = Request::new(ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(OtelResource {
                    attributes: vec![KeyValue {
                        key: "res_key".into(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("res_val".into())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                scope_logs: vec![ScopeLogs {
                    scope: Some(InstrumentationScope {
                        name: "some.scope.name".into(),
                        version: "1.2.3".into(),
                        attributes: vec![KeyValue {
                            key: "scope_attr".into(),
                            value: Some(AnyValue {
                                value: Some(any_value::Value::StringValue("scope_val".into())),
                            }),
                        }],
                        dropped_attributes_count: 7,
                    }),
                    log_records: vec![LogRecord {
                        time_unix_nano: 1,
                        observed_time_unix_nano: 2,
                        severity_number: 9,
                        severity_text: "info".into(),
                        body: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("log body".into())),
                        }),
                        attributes: vec![KeyValue {
                            key: "attr_key".into(),
                            value: Some(AnyValue {
                                value: Some(any_value::Value::StringValue("attr_val".into())),
                            }),
                        }],
                        dropped_attributes_count: 3,
                        flags: 4,
                        // opentelemetry sdk will hex::decode the given trace_id and span_id
                        trace_id: str_into_hex_bytes("4ac52aadf321c2e531db005df08792f5"),
                        span_id: str_into_hex_bytes("0b9e4bda2a55530d"),
                    }],
                    schema_url: "v1".into(),
                }],
                schema_url: "v1".into(),
            }],
        });
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(logs_output).await;
        // we just send one, so only one output
        assert_eq!(output.len(), 1);
        let actual_event = output.pop().unwrap();
        schema_definitions
            .unwrap()
            .assert_valid_for_event(&actual_event);
        let expect_vec = vec_into_btmap(vec![
            (
                "attributes",
                Value::Object(vec_into_btmap(vec![("attr_key", "attr_val".into())])),
            ),
            (
                "resources",
                Value::Object(vec_into_btmap(vec![("res_key", "res_val".into())])),
            ),
            (
                "scope",
                Value::Object(vec_into_btmap(vec![
                    ("name", "some.scope.name".into()),
                    ("version", "1.2.3".into()),
                    (
                        "attributes",
                        Value::Object(vec_into_btmap(vec![("scope_attr", "scope_val".into())])),
                    ),
                    ("dropped_attributes_count", 7.into()),
                ])),
            ),
            ("message", "log body".into()),
            ("trace_id", "4ac52aadf321c2e531db005df08792f5".into()),
            ("span_id", "0b9e4bda2a55530d".into()),
            ("severity_number", 9.into()),
            ("severity_text", "info".into()),
            ("flags", 4.into()),
            ("dropped_attributes_count", 3.into()),
            ("timestamp", Utc.timestamp_nanos(1).into()),
            ("observed_timestamp", Utc.timestamp_nanos(2).into()),
            ("source_type", "opentelemetry".into()),
        ]);
        let mut expect_event = Event::from(LogEvent::from(expect_vec));
        expect_event.set_upstream_id(Arc::new(OutputId {
            component: "test".into(),
            port: Some("logs".into()),
        }));
        assert_eq!(actual_event, expect_event);
    })
    .await;
}

pub(super) fn new_source(
    status: EventStatus,
    event_name: String,
) -> (
    SourceSender,
    impl Stream<Item = Event>,
    impl Stream<Item = Event>,
) {
    let (mut sender, recv) = SourceSender::new_test_finalize(status);
    let output = sender
        .add_outputs(status, event_name)
        .flat_map(into_event_stream);
    (sender, output, recv)
}

fn str_into_hex_bytes(s: &str) -> Vec<u8> {
    // unwrap is okay in test
    hex::decode(s).unwrap()
}

fn vec_into_btmap(arr: Vec<(&'static str, Value)>) -> ObjectMap {
    ObjectMap::from_iter(
        arr.into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect::<Vec<(_, _)>>(),
    )
}
