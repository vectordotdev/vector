use std::{
    net,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    SourceSender,
    config::{OutputId, SourceConfig, SourceContext},
    event::{
        Event, EventStatus, LogEvent, Metric as MetricEvent, MetricKind, MetricTags, MetricValue,
        ObjectMap, TraceLayout, Value, into_event_stream,
        metric::{Bucket, Quantile},
    },
    sources::opentelemetry::config::{
        GrpcConfig, HttpConfig, LOGS, METRICS, OpentelemetryConfig, TRACES,
    },
    test_util::{
        self,
        addr::next_addr,
        components::{SOURCE_TAGS, assert_source_compliance},
    },
};
use chrono::{DateTime, TimeZone, Utc};
use futures::Stream;
use futures_util::StreamExt;
use prost::Message;
use similar_asserts::assert_eq;
use tokio::sync::Semaphore;
use tonic::Request;
use vector_lib::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;
use vector_lib::opentelemetry::proto::trace::v1::{ResourceSpans, ScopeSpans, Span};
use vector_lib::{
    config::LogNamespace,
    lookup::path,
    opentelemetry::proto::{
        collector::{
            logs::v1::{ExportLogsServiceRequest, logs_service_client::LogsServiceClient},
            metrics::v1::{
                ExportMetricsServiceRequest, metrics_service_client::MetricsServiceClient,
            },
            trace::v1::trace_service_client::TraceServiceClient,
        },
        common::v1::{AnyValue, InstrumentationScope, KeyValue, any_value::Value::StringValue},
        logs::v1::{LogRecord, ResourceLogs, ScopeLogs},
        metrics::v1::{
            AggregationTemporality, ExponentialHistogram, ExponentialHistogramDataPoint, Gauge,
            Histogram, HistogramDataPoint, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics,
            Sum, Summary, SummaryDataPoint, exponential_histogram_data_point::Buckets,
            metric::Data, summary_data_point::ValueAtQuantile,
        },
        resource::v1::{Resource, Resource as OtelResource},
    },
};
use vrl::{event_path, value};

fn create_test_logs_request() -> Request<ExportLogsServiceRequest> {
    Request::new(ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(OtelResource {
                attributes: vec![KeyValue {
                    key: "res_key".into(),
                    value: Some(AnyValue {
                        value: Some(StringValue("res_val".into())),
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
                            value: Some(StringValue("scope_val".into())),
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
                        value: Some(StringValue("log body".into())),
                    }),
                    attributes: vec![KeyValue {
                        key: "attr_key".into(),
                        value: Some(AnyValue {
                            value: Some(StringValue("attr_val".into())),
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
    })
}

fn create_test_metrics_request() -> ExportMetricsServiceRequest {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "service.name".to_string(),
                    value: Some(AnyValue {
                        value: Some(StringValue("vector-collector".to_string())),
                    }),
                }],
                dropped_attributes_count: 0,
            }),
            schema_url: "".to_string(),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(InstrumentationScope {
                    name: "vector-collector-instrumentation".to_string(),
                    version: "0.111.0".to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                schema_url: "".to_string(),
                metrics: vec![Metric {
                    name: "some.random.metric".to_string(),
                    description: "Some random metric we use for test".to_string(),
                    unit: "1".to_string(),
                    metadata: vec![],
                    data: Some(Data::Summary(Summary {
                        data_points: vec![SummaryDataPoint {
                            attributes: vec![
                                KeyValue {
                                    key: "host".to_string(),
                                    value: Some(AnyValue {
                                        value: Some(StringValue("localhost".to_string())),
                                    }),
                                },
                                KeyValue {
                                    key: "service".to_string(),
                                    value: Some(AnyValue {
                                        value: Some(StringValue("vector-collector".to_string())),
                                    }),
                                },
                            ],
                            start_time_unix_nano: 0,
                            time_unix_nano: 0,
                            count: 5,
                            sum: 122.5,
                            quantile_values: vec![
                                ValueAtQuantile {
                                    quantile: 0.5,
                                    value: 24.5,
                                },
                                ValueAtQuantile {
                                    quantile: 0.9,
                                    value: 45.0,
                                },
                                ValueAtQuantile {
                                    quantile: 1.0,
                                    value: 60.0,
                                },
                            ],
                            flags: 0,
                        }],
                    })),
                }],
            }],
        }],
    }
}

fn create_test_traces_request() -> ExportTraceServiceRequest {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: None,
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![Span {
                    trace_id: (1..17).collect::<Vec<u8>>(),
                    span_id: (1..9).collect::<Vec<u8>>(),
                    parent_span_id: (1..9).collect::<Vec<u8>>(),
                    flags: 0,
                    name: "span".to_string(),
                    kind: 1,
                    start_time_unix_nano: 1713525203000000000,
                    end_time_unix_nano: 1713525205000000000,
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    events: vec![],
                    dropped_events_count: 0,
                    links: vec![],
                    dropped_links_count: 0,
                    status: None,
                    trace_state: "".to_string(),
                }],
                schema_url: "".to_string(),
            }],
            schema_url: "".to_string(),
        }],
    }
}

#[test]
fn generate_config() {
    test_util::test_generate_config::<OpentelemetryConfig>();
}

#[test]
fn admission_config_defaults_and_rejects_invalid_values() {
    let yaml = "grpc:\n  address: 0.0.0.0:4317\nhttp:\n  address: 0.0.0.0:4318\n";
    let config: OpentelemetryConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.max_concurrent_requests, None);
    assert_eq!(config.request_timeout_secs, None);
    let serialized = serde_json::to_value(&config).unwrap();
    assert!(serialized.get("max_concurrent_requests").is_none());
    assert!(serialized.get("request_timeout_secs").is_none());

    let configured: OpentelemetryConfig = serde_yaml::from_str(&format!(
        "{yaml}max_concurrent_requests: 7\nrequest_timeout_secs: 11\n"
    ))
    .unwrap();
    assert_eq!(configured.max_concurrent_requests.unwrap().get(), 7);
    assert_eq!(configured.request_timeout_secs, Some(11));
    let round_trip: OpentelemetryConfig =
        serde_json::from_value(serde_json::to_value(&configured).unwrap()).unwrap();
    assert_eq!(
        round_trip.max_concurrent_requests,
        configured.max_concurrent_requests
    );
    assert_eq!(
        round_trip.request_timeout_secs,
        configured.request_timeout_secs
    );

    let limit_only: OpentelemetryConfig =
        serde_yaml::from_str(&format!("{yaml}max_concurrent_requests: 7\n")).unwrap();
    assert_eq!(limit_only.max_concurrent_requests.unwrap().get(), 7);
    assert_eq!(limit_only.request_timeout_secs, None);
    let timeout_only: OpentelemetryConfig =
        serde_yaml::from_str(&format!("{yaml}request_timeout_secs: 11\n")).unwrap();
    assert_eq!(timeout_only.max_concurrent_requests, None);
    assert_eq!(timeout_only.request_timeout_secs, Some(11));
    let nulls: OpentelemetryConfig = serde_yaml::from_str(&format!(
        "{yaml}max_concurrent_requests: null\nrequest_timeout_secs: null\n"
    ))
    .unwrap();
    assert_eq!(nulls.max_concurrent_requests, None);
    assert_eq!(nulls.request_timeout_secs, None);

    for limit in [0, Semaphore::MAX_PERMITS + 1] {
        let yaml = format!("{yaml}max_concurrent_requests: {limit}\n");
        assert!(serde_yaml::from_str::<OpentelemetryConfig>(&yaml).is_err());
    }
}

#[test]
fn config_grpc_keepalive() {
    let config: OpentelemetryConfig = toml::from_str(
        r#"
            [grpc]
            address = "0.0.0.0:4317"

            [grpc.keepalive]
            max_connection_age_secs = 300
            max_connection_age_grace_secs = 30

            [http]
            address = "0.0.0.0:4318"
        "#,
    )
    .unwrap();

    assert_eq!(config.grpc.keepalive.max_connection_age_secs, Some(300));
    assert_eq!(
        config.grpc.keepalive.max_connection_age_grace_secs,
        Some(30)
    );
}

#[tokio::test]
async fn http_and_grpc_acknowledgement_waits_do_not_hold_admission_or_time_out() {
    for concurrency_limit in [None, Some(1)] {
        for timeout in [None, Some(1)] {
            check_acknowledgement_waits(concurrency_limit, timeout).await;
        }
    }
}

async fn check_acknowledgement_waits(concurrency_limit: Option<usize>, timeout: Option<u64>) {
    let (_guard_0, grpc_addr) = next_addr();
    let (_guard_1, http_addr) = next_addr();
    let mut config = get_source_config_with_headers(grpc_addr, http_addr, false);
    config.acknowledgements = true.into();
    config.max_concurrent_requests = concurrency_limit.map(|limit| limit.try_into().unwrap());
    config.request_timeout_secs = timeout;

    let (sender, mut output) = new_unacknowledged_logs_source(&config);
    let server = config
        .build(SourceContext::new_test(sender, None))
        .await
        .unwrap();
    tokio::spawn(server);
    test_util::wait_for_tcp(http_addr).await;
    test_util::wait_for_tcp(grpc_addr).await;

    let deadline = std::time::Duration::from_secs(5);
    let mut requests = Vec::new();
    let mut events = Vec::new();
    let http_client = reqwest::Client::new();
    for _ in 0..2 {
        let request = http_client
            .post(format!("http://{http_addr}/v1/logs"))
            .header("Content-Type", "application/x-protobuf")
            .body(create_test_logs_request().into_inner().encode_to_vec());
        requests.push(tokio::spawn(async move {
            assert_eq!(
                request.send().await.unwrap().status(),
                reqwest::StatusCode::OK
            );
        }));
        // Retaining the event's finalizer keeps the request waiting for acknowledgement.
        events.push(
            tokio::time::timeout(deadline, output.next())
                .await
                .unwrap()
                .unwrap(),
        );
    }

    let grpc_client = LogsServiceClient::connect(format!("http://{grpc_addr}"))
        .await
        .unwrap();
    // Cloned clients multiplex exports over the same HTTP/2 connection.
    for _ in 0..2 {
        let mut client = grpc_client.clone();
        requests.push(tokio::spawn(async move {
            client.export(create_test_logs_request()).await.unwrap();
        }));
        events.push(
            tokio::time::timeout(deadline, output.next())
                .await
                .unwrap()
                .unwrap(),
        );
    }

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    assert!(requests.iter().all(|request| !request.is_finished()));

    for event in events {
        event
            .metadata()
            .finalizers()
            .update_status(EventStatus::Delivered);
    }
    for request in requests {
        tokio::time::timeout(deadline, request)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn acknowledgement_status_is_reported_over_http_and_grpc() {
    let (_guard_0, grpc_addr) = next_addr();
    let (_guard_1, http_addr) = next_addr();
    let mut config = get_source_config_with_headers(grpc_addr, http_addr, false);
    config.acknowledgements = true.into();

    let (sender, mut output) = new_unacknowledged_logs_source(&config);
    let server = config
        .build(SourceContext::new_test(sender, None))
        .await
        .unwrap();
    tokio::spawn(server);
    test_util::wait_for_tcp(http_addr).await;
    test_util::wait_for_tcp(grpc_addr).await;

    let client = reqwest::Client::new();
    let grpc_client = LogsServiceClient::connect(format!("http://{grpc_addr}"))
        .await
        .unwrap();
    let deadline = std::time::Duration::from_secs(5);
    for (status, expected_code, expected_message) in [
        (EventStatus::Delivered, tonic::Code::Ok, ""),
        (
            EventStatus::Errored,
            tonic::Code::Internal,
            "Error delivering contents to sink",
        ),
        (
            EventStatus::Rejected,
            tonic::Code::DataLoss,
            "Contents failed to deliver to sink",
        ),
    ] {
        let http_request = tokio::spawn({
            let client = client.clone();
            async move {
                client
                    .post(format!("http://{http_addr}/v1/logs"))
                    .header("Content-Type", "application/x-protobuf")
                    .body(create_test_logs_request().into_inner().encode_to_vec())
                    .send()
                    .await
                    .unwrap()
            }
        });
        let event = tokio::time::timeout(deadline, output.next())
            .await
            .unwrap()
            .unwrap();
        event.metadata().finalizers().update_status(status);
        drop(event);
        let response = tokio::time::timeout(deadline, http_request)
            .await
            .unwrap()
            .unwrap();
        if expected_code == tonic::Code::Ok {
            assert_eq!(response.status(), reqwest::StatusCode::OK);
        } else {
            assert_eq!(
                response.status(),
                reqwest::StatusCode::INTERNAL_SERVER_ERROR
            );
            let status = super::status::Status::decode(response.bytes().await.unwrap()).unwrap();
            assert_eq!(status.code, tonic::Code::Unknown as i32);
            assert_eq!(status.message, expected_message);
        }

        let grpc_request = tokio::spawn({
            let mut client = grpc_client.clone();
            async move { client.export(create_test_logs_request()).await }
        });
        let event = tokio::time::timeout(deadline, output.next())
            .await
            .unwrap()
            .unwrap();
        event.metadata().finalizers().update_status(status);
        drop(event);
        let response = tokio::time::timeout(deadline, grpc_request)
            .await
            .unwrap()
            .unwrap();
        if expected_code == tonic::Code::Ok {
            response.unwrap();
        } else {
            assert_eq!(response.unwrap_err().code(), expected_code);
        }
    }
}

#[tokio::test]
async fn receive_grpc_logs_vector_namespace() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let env = build_otlp_test_env(LOGS, Some(true)).await;
        let schema_definitions = env
            .config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        // send request via grpc client
        let mut client = LogsServiceClient::connect(format!("http://{}", env.grpc_addr))
            .await
            .unwrap();
        let req = create_test_logs_request();
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(env.output);
        // we just send one, so only one output
        assert_eq!(output.len(), 1);
        let event = output.pop().unwrap();
        schema_definitions.unwrap().assert_valid_for_event(&event);

        assert_eq!(
            event.as_log().get(event_path!()).unwrap(),
            &value!("log body")
        );

        let meta = event.as_log().metadata().value();
        assert_eq!(
            meta.get(path!("vector", "source_type")).unwrap(),
            &value!(OpentelemetryConfig::NAME)
        );
        assert!(
            meta.get(path!("vector", "ingest_timestamp"))
                .unwrap()
                .is_timestamp()
        );
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
            meta.get(path!("opentelemetry", "scope", "version"))
                .unwrap(),
            &value!("1.2.3")
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "scope", "attributes"))
                .unwrap(),
            &value!({scope_attr: "scope_val"})
        );
        assert_eq!(
            meta.get(path!("opentelemetry", "scope", "dropped_attributes_count"))
                .unwrap(),
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
        let env = build_otlp_test_env(LOGS, None).await;
        let schema_definitions = env
            .config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true)
            .unwrap();

        // send request via grpc client
        let mut client = LogsServiceClient::connect(format!("http://{}", env.grpc_addr))
            .await
            .unwrap();
        let req = create_test_logs_request();
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(env.output);
        // we just send one, so only one output
        assert_eq!(output.len(), 1);
        let actual_event = output.pop().unwrap();
        schema_definitions.assert_valid_for_event(&actual_event);
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

#[tokio::test]
async fn receive_sum_metric() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let env = build_otlp_test_env(METRICS, None).await;

        // send request via grpc client
        let mut client = MetricsServiceClient::connect(format!("http://{}", env.grpc_addr))
            .await
            .unwrap();
        let (event_time, event_time_nanos) = current_time_and_nanos();
        let req = Request::new(ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(StringValue("vector-collector".to_string())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                schema_url: "".to_string(),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        name: "vector-collector-instrumentation".to_string(),
                        version: "0.111.0".to_string(),
                        attributes: vec![],
                        dropped_attributes_count: 0,
                    }),
                    schema_url: "".to_string(),
                    metrics: vec![Metric {
                        name: "some.random.metric".to_string(),
                        description: "Some random metric we use for test".to_string(),
                        unit: "1".to_string(),
                        metadata: vec![],
                        data: Some(Data::Sum(Sum {
                            data_points: vec![NumberDataPoint {
                                attributes: vec![
                                    KeyValue {
                                        key: "host".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue("localhost".to_string())),
                                        }),
                                    }, KeyValue {
                                        key: "service".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue("vector-collector".to_string())),
                                        }),
                                    },
                                ],
                                start_time_unix_nano: 0,
                                time_unix_nano: event_time_nanos,
                                exemplars: vec![],
                                flags: 0,
                                value: Some(vector_lib::opentelemetry::proto::metrics::v1::number_data_point::Value::AsDouble(42.0)),
                            }],
                            aggregation_temporality: AggregationTemporality::Cumulative as i32,
                            // monotonic =  incremental
                            is_monotonic: true,
                        })),
                    }],
                }],
            }],
        });
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(env.output);
        assert_eq!(output.len(), 1);
        let actual_event = output.pop().unwrap();

        let mut tags = MetricTags::default();
        tags.insert("resource.service.name".to_string(),"vector-collector".to_string());
        tags.insert("scope.name".to_string(), "vector-collector-instrumentation".to_string());
        tags.insert("scope.version".to_string(), "0.111.0".to_string());
        tags.insert("host".to_string(), "localhost".to_string());
        tags.insert("service".to_string(), "vector-collector".to_string());

        let mut expected_event = Event::from(MetricEvent::new(
            "some.random.metric",
            MetricKind::Absolute, // since monotonic = true
            MetricValue::Counter { value: 42.0 },
        )
            .with_timestamp(Some(DateTime::<Utc>::from(event_time)))
            .with_tags(Some(tags)));
        expected_event.set_upstream_id(Arc::new(OutputId {
            component: "test".into(),
            port: Some("metrics".into()),
        }));
        assert_eq!(actual_event, expected_event);
    })
        .await;
}

#[tokio::test]
async fn receive_sum_non_monotonic_metric() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let env = build_otlp_test_env(METRICS, None).await;

        // send request via grpc client
        let mut client = MetricsServiceClient::connect(format!("http://{}", env.grpc_addr))
            .await
            .unwrap();
        let (event_time, event_time_nanos) = current_time_and_nanos();

        let req = Request::new(ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(StringValue("vector-collector".to_string())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                schema_url: "".to_string(),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        name: "vector-collector-instrumentation".to_string(),
                        version: "0.111.0".to_string(),
                        attributes: vec![],
                        dropped_attributes_count: 0,
                    }),
                    schema_url: "".to_string(),
                    metrics: vec![Metric {
                        name: "some.random.metric".to_string(),
                        description: "Some random metric we use for test".to_string(),
                        unit: "1".to_string(),
                        metadata: vec![],
                        data: Some(Data::Sum(Sum {
                            data_points: vec![NumberDataPoint {
                                attributes: vec![
                                    KeyValue {
                                        key: "host".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue("localhost".to_string())),
                                        }),
                                    }, KeyValue {
                                        key: "service".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue("vector-collector".to_string())),
                                        }),
                                    },
                                ],
                                start_time_unix_nano: 0,
                                time_unix_nano: event_time_nanos,
                                exemplars: vec![],
                                flags: 0,
                                value: Some(vector_lib::opentelemetry::proto::metrics::v1::number_data_point::Value::AsDouble(42.0)),
                            }],
                            aggregation_temporality: AggregationTemporality::Cumulative as i32,
                            // monotonic =  incremental
                            is_monotonic: false,
                        })),
                    }],
                }],
            }],
        });
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(env.output);
        assert_eq!(output.len(), 1);
        let actual_event = output.pop().unwrap();

        let mut tags = MetricTags::default();
        tags.insert("resource.service.name".to_string(),"vector-collector".to_string());
        tags.insert("scope.name".to_string(), "vector-collector-instrumentation".to_string());
        tags.insert("scope.version".to_string(), "0.111.0".to_string());
        tags.insert("host".to_string(), "localhost".to_string());
        tags.insert("service".to_string(), "vector-collector".to_string());

        let mut expected_event = Event::from(MetricEvent::new(
            "some.random.metric",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 42.0 }, // since we have monotonic = false
        )
            .with_timestamp(Some(DateTime::<Utc>::from(event_time)))
            .with_tags(Some(tags)));
        expected_event.set_upstream_id(Arc::new(OutputId {
            component: "test".into(),
            port: Some("metrics".into()),
        }));
        assert_eq!(actual_event, expected_event);
    })
        .await;
}

#[tokio::test]
async fn receive_gauge_metric() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let env = build_otlp_test_env(METRICS, None).await;

        // send request via grpc client
        let mut client = MetricsServiceClient::connect(format!("http://{}", env.grpc_addr))
            .await
            .unwrap();
        let (event_time, event_time_nanos) = current_time_and_nanos();

        let req = Request::new(ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(StringValue("vector-collector".to_string())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                schema_url: "".to_string(),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        name: "vector-collector-instrumentation".to_string(),
                        version: "0.111.0".to_string(),
                        attributes: vec![],
                        dropped_attributes_count: 0,
                    }),
                    schema_url: "".to_string(),
                    metrics: vec![Metric {
                        name: "some.random.metric".to_string(),
                        description: "Some random metric we use for test".to_string(),
                        unit: "1".to_string(),
                        metadata: vec![],
                        data: Some(Data::Gauge(Gauge {
                            data_points: vec![NumberDataPoint {
                                attributes: vec![
                                    KeyValue {
                                        key: "host".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue("localhost".to_string())),
                                        }),
                                    }, KeyValue {
                                        key: "service".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue("vector-collector".to_string())),
                                        }),
                                    },
                                ],
                                start_time_unix_nano: 0,
                                time_unix_nano: event_time_nanos,
                                exemplars: vec![],
                                flags: 0,
                                value: Some(vector_lib::opentelemetry::proto::metrics::v1::number_data_point::Value::AsDouble(42.0)),
                            }],
                        })),
                    }],
                }],
            }],
        });
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(env.output);
        assert_eq!(output.len(), 1);
        let actual_event = output.pop().unwrap();

        let mut tags = MetricTags::default();
        tags.insert("resource.service.name".to_string(),"vector-collector".to_string());
        tags.insert("scope.name".to_string(), "vector-collector-instrumentation".to_string());
        tags.insert("scope.version".to_string(), "0.111.0".to_string());
        tags.insert("host".to_string(), "localhost".to_string());
        tags.insert("service".to_string(), "vector-collector".to_string());

        let mut expected_event = Event::from(MetricEvent::new(
            "some.random.metric",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 42.0 },
        )
            .with_timestamp(Some(DateTime::<Utc>::from(event_time)))
            .with_tags(Some(tags)));
        expected_event.set_upstream_id(Arc::new(OutputId {
            component: "test".into(),
            port: Some("metrics".into()),
        }));
        assert_eq!(actual_event, expected_event);
    })
        .await;
}

#[tokio::test]
async fn receive_histogram_metric() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let env = build_otlp_test_env(METRICS, None).await;

        // send request via grpc client
        let mut client = MetricsServiceClient::connect(format!("http://{}", env.grpc_addr))
            .await
            .unwrap();
        let (event_time, event_time_nanos) = current_time_and_nanos();

        let req = Request::new(ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(StringValue("vector-collector".to_string())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                schema_url: "".to_string(),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        name: "vector-collector-instrumentation".to_string(),
                        version: "0.111.0".to_string(),
                        attributes: vec![],
                        dropped_attributes_count: 0,
                    }),
                    schema_url: "".to_string(),
                    metrics: vec![Metric {
                        name: "some.random.metric".to_string(),
                        description: "Some random metric we use for test".to_string(),
                        unit: "1".to_string(),
                        metadata: vec![],
                        data: Some(Data::Histogram(Histogram {
                            aggregation_temporality: AggregationTemporality::Cumulative as i32,
                            data_points: vec![HistogramDataPoint {
                                attributes: vec![
                                    KeyValue {
                                        key: "host".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue("localhost".to_string())),
                                        }),
                                    },
                                    KeyValue {
                                        key: "service".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue(
                                                "vector-collector".to_string(),
                                            )),
                                        }),
                                    },
                                ],
                                start_time_unix_nano: 0,
                                time_unix_nano: event_time_nanos,
                                count: 9,
                                sum: Some(123.45),
                                bucket_counts: vec![1, 2, 2, 4],
                                explicit_bounds: vec![50.0, 100.0, 150.0],
                                exemplars: vec![],
                                flags: 0,
                                min: Some(10.0),
                                max: Some(60.0),
                            }],
                        })),
                    }],
                }],
            }],
        });
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(env.output);
        assert_eq!(output.len(), 1);
        let actual_event = output.pop().unwrap();

        let mut tags = MetricTags::default();
        tags.insert(
            "resource.service.name".to_string(),
            "vector-collector".to_string(),
        );
        tags.insert(
            "scope.name".to_string(),
            "vector-collector-instrumentation".to_string(),
        );
        tags.insert("scope.version".to_string(), "0.111.0".to_string());
        tags.insert("host".to_string(), "localhost".to_string());
        tags.insert("service".to_string(), "vector-collector".to_string());

        let mut expected_event = Event::from(
            MetricEvent::new(
                "some.random.metric",
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    buckets: vec![
                        Bucket {
                            count: 1,
                            upper_limit: 50.0,
                        },
                        Bucket {
                            count: 2,
                            upper_limit: 100.0,
                        },
                        Bucket {
                            count: 2,
                            upper_limit: 150.0,
                        },
                        Bucket {
                            count: 4,
                            upper_limit: f64::INFINITY,
                        },
                    ],
                    count: 9,
                    sum: 123.45,
                },
            )
            .with_timestamp(Some(DateTime::<Utc>::from(event_time)))
            .with_tags(Some(tags)),
        );
        expected_event.set_upstream_id(Arc::new(OutputId {
            component: "test".into(),
            port: Some("metrics".into()),
        }));
        assert_eq!(actual_event, expected_event);
    })
    .await;
}

#[tokio::test]
async fn receive_histogram_delta_metric() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let env = build_otlp_test_env(METRICS, None).await;

        // send request via grpc client
        let mut client = MetricsServiceClient::connect(format!("http://{}", env.grpc_addr))
            .await
            .unwrap();
        let (event_time, event_time_nanos) = current_time_and_nanos();

        let req = Request::new(ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(StringValue("vector-collector".to_string())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                schema_url: "".to_string(),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        name: "vector-collector-instrumentation".to_string(),
                        version: "0.111.0".to_string(),
                        attributes: vec![],
                        dropped_attributes_count: 0,
                    }),
                    schema_url: "".to_string(),
                    metrics: vec![Metric {
                        name: "some.random.metric".to_string(),
                        description: "Some random metric we use for test".to_string(),
                        unit: "1".to_string(),
                        metadata: vec![],
                        data: Some(Data::Histogram(Histogram {
                            aggregation_temporality: AggregationTemporality::Delta as i32,
                            data_points: vec![HistogramDataPoint {
                                attributes: vec![
                                    KeyValue {
                                        key: "host".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue("localhost".to_string())),
                                        }),
                                    },
                                    KeyValue {
                                        key: "service".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue(
                                                "vector-collector".to_string(),
                                            )),
                                        }),
                                    },
                                ],
                                start_time_unix_nano: 0,
                                time_unix_nano: event_time_nanos,
                                count: 9,
                                sum: Some(123.45),
                                bucket_counts: vec![1, 2, 2, 4],
                                explicit_bounds: vec![50.0, 100.0, 150.0],
                                exemplars: vec![],
                                flags: 0,
                                min: Some(10.0),
                                max: Some(60.0),
                            }],
                        })),
                    }],
                }],
            }],
        });
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(env.output);
        assert_eq!(output.len(), 1);
        let actual_event = output.pop().unwrap();

        let mut tags = MetricTags::default();
        tags.insert(
            "resource.service.name".to_string(),
            "vector-collector".to_string(),
        );
        tags.insert(
            "scope.name".to_string(),
            "vector-collector-instrumentation".to_string(),
        );
        tags.insert("scope.version".to_string(), "0.111.0".to_string());
        tags.insert("host".to_string(), "localhost".to_string());
        tags.insert("service".to_string(), "vector-collector".to_string());

        let mut expected_event = Event::from(
            MetricEvent::new(
                "some.random.metric",
                MetricKind::Incremental,
                MetricValue::AggregatedHistogram {
                    buckets: vec![
                        Bucket {
                            count: 1,
                            upper_limit: 50.0,
                        },
                        Bucket {
                            count: 2,
                            upper_limit: 100.0,
                        },
                        Bucket {
                            count: 2,
                            upper_limit: 150.0,
                        },
                        Bucket {
                            count: 4,
                            upper_limit: f64::INFINITY,
                        },
                    ],
                    count: 9,
                    sum: 123.45,
                },
            )
            .with_timestamp(Some(DateTime::<Utc>::from(event_time)))
            .with_tags(Some(tags)),
        );
        expected_event.set_upstream_id(Arc::new(OutputId {
            component: "test".into(),
            port: Some("metrics".into()),
        }));
        assert_eq!(actual_event, expected_event);
    })
    .await;
}

#[tokio::test]
async fn receive_exponential_histogram_metric() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let env = build_otlp_test_env(METRICS, None).await;

        // send request via grpc client
        let mut client = MetricsServiceClient::connect(format!("http://{}", env.grpc_addr))
            .await
            .unwrap();
        let (event_time, event_time_nanos) = current_time_and_nanos();

        let req = Request::new(ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(StringValue("vector-collector".to_string())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                schema_url: "".to_string(),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        name: "vector-collector-instrumentation".to_string(),
                        version: "0.111.0".to_string(),
                        attributes: vec![],
                        dropped_attributes_count: 0,
                    }),
                    schema_url: "".to_string(),
                    metrics: vec![Metric {
                        name: "some.random.metric".to_string(),
                        description: "Some random metric we use for test".to_string(),
                        unit: "1".to_string(),
                        metadata: vec![],
                        data: Some(Data::ExponentialHistogram(ExponentialHistogram {
                            aggregation_temporality: AggregationTemporality::Cumulative as i32,
                            data_points: vec![ExponentialHistogramDataPoint {
                                attributes: vec![
                                    KeyValue {
                                        key: "host".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue("localhost".to_string())),
                                        }),
                                    },
                                    KeyValue {
                                        key: "service".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue(
                                                "vector-collector".to_string(),
                                            )),
                                        }),
                                    },
                                ],
                                start_time_unix_nano: 0,
                                time_unix_nano: event_time_nanos,
                                count: 7,
                                sum: Some(700.0),
                                scale: 2,
                                zero_count: 1,
                                positive: Some(Buckets {
                                    offset: 0,
                                    bucket_counts: vec![2, 1],
                                }),
                                negative: Some(Buckets {
                                    offset: -1,
                                    bucket_counts: vec![1, 2],
                                }),
                                min: Some(-120.0),
                                max: Some(150.0),
                                exemplars: vec![],
                                flags: 0,
                                zero_threshold: 0f64,
                            }],
                        })),
                    }],
                }],
            }],
        });
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(env.output);
        assert_eq!(output.len(), 1);
        let actual_event = output.pop().unwrap();

        let mut tags = MetricTags::default();
        tags.insert(
            "resource.service.name".to_string(),
            "vector-collector".to_string(),
        );
        tags.insert(
            "scope.name".to_string(),
            "vector-collector-instrumentation".to_string(),
        );
        tags.insert("scope.version".to_string(), "0.111.0".to_string());
        tags.insert("host".to_string(), "localhost".to_string());
        tags.insert("service".to_string(), "vector-collector".to_string());

        let mut expected_event = Event::from(
            MetricEvent::new(
                "some.random.metric",
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    buckets: vec![
                        Bucket {
                            count: 1,
                            upper_limit: -0.8408964152537146,
                        },
                        Bucket {
                            count: 2,
                            upper_limit: -1.0,
                        },
                        Bucket {
                            count: 1,
                            upper_limit: 0f64,
                        },
                        Bucket {
                            count: 2,
                            upper_limit: 1.189207115002721,
                        },
                        Bucket {
                            count: 1,
                            upper_limit: 1.4142135623730951,
                        },
                    ],
                    count: 7,
                    sum: 700.00,
                },
            )
            .with_timestamp(Some(DateTime::<Utc>::from(event_time)))
            .with_tags(Some(tags)),
        );
        expected_event.set_upstream_id(Arc::new(OutputId {
            component: "test".into(),
            port: Some("metrics".into()),
        }));
        assert_eq!(actual_event, expected_event);
    })
    .await;
}

#[tokio::test]
async fn receive_summary_metric() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let env = build_otlp_test_env(METRICS, None).await;

        // send request via grpc client
        let mut client = MetricsServiceClient::connect(format!("http://{}", env.grpc_addr))
            .await
            .unwrap();
        let (event_time, event_time_nanos) = current_time_and_nanos();

        let req = Request::new(ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(StringValue("vector-collector".to_string())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                schema_url: "".to_string(),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        name: "vector-collector-instrumentation".to_string(),
                        version: "0.111.0".to_string(),
                        attributes: vec![],
                        dropped_attributes_count: 0,
                    }),
                    schema_url: "".to_string(),
                    metrics: vec![Metric {
                        name: "some.random.metric".to_string(),
                        description: "Some random metric we use for test".to_string(),
                        unit: "1".to_string(),
                        metadata: vec![],
                        data: Some(Data::Summary(Summary {
                            data_points: vec![SummaryDataPoint {
                                attributes: vec![
                                    KeyValue {
                                        key: "host".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue("localhost".to_string())),
                                        }),
                                    },
                                    KeyValue {
                                        key: "service".to_string(),
                                        value: Some(AnyValue {
                                            value: Some(StringValue(
                                                "vector-collector".to_string(),
                                            )),
                                        }),
                                    },
                                ],
                                start_time_unix_nano: 0,
                                time_unix_nano: event_time_nanos,
                                count: 5,
                                sum: 122.5,
                                quantile_values: vec![
                                    ValueAtQuantile {
                                        quantile: 0.5,
                                        value: 24.5,
                                    },
                                    ValueAtQuantile {
                                        quantile: 0.9,
                                        value: 45.0,
                                    },
                                    ValueAtQuantile {
                                        quantile: 1.0,
                                        value: 60.0,
                                    },
                                ],
                                flags: 0,
                            }],
                        })),
                    }],
                }],
            }],
        });
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(env.output);
        assert_eq!(output.len(), 1);
        let actual_event = output.pop().unwrap();

        let mut tags = MetricTags::default();
        tags.insert(
            "resource.service.name".to_string(),
            "vector-collector".to_string(),
        );
        tags.insert(
            "scope.name".to_string(),
            "vector-collector-instrumentation".to_string(),
        );
        tags.insert("scope.version".to_string(), "0.111.0".to_string());
        tags.insert("host".to_string(), "localhost".to_string());
        tags.insert("service".to_string(), "vector-collector".to_string());

        let mut expected_event = Event::from(
            MetricEvent::new(
                "some.random.metric",
                MetricKind::Absolute,
                MetricValue::AggregatedSummary {
                    quantiles: vec![
                        Quantile {
                            quantile: 0.5,
                            value: 24.5,
                        },
                        Quantile {
                            quantile: 0.9,
                            value: 45.0,
                        },
                        Quantile {
                            quantile: 1.0,
                            value: 60.0,
                        },
                    ],
                    count: 5,
                    sum: 122.5,
                },
            )
            .with_timestamp(Some(DateTime::<Utc>::from(event_time)))
            .with_tags(Some(tags)),
        );
        expected_event.set_upstream_id(Arc::new(OutputId {
            component: "test".into(),
            port: Some("metrics".into()),
        }));
        assert_eq!(actual_event, expected_event);
    })
    .await;
}

fn get_source_config_with_headers(
    grpc_addr: net::SocketAddr,
    http_addr: net::SocketAddr,
    use_otlp_decoding: bool,
) -> OpentelemetryConfig {
    OpentelemetryConfig {
        grpc: GrpcConfig {
            address: grpc_addr,
            tls: Default::default(),
            keepalive: Default::default(),
        },
        http: HttpConfig {
            address: http_addr,
            tls: Default::default(),
            keepalive: Default::default(),
            headers: vec![
                "User-Agent".to_string(),
                "X-*".to_string(),
                "AbsentHeader".to_string(),
            ],
        },
        acknowledgements: Default::default(),
        max_concurrent_requests: None,
        request_timeout_secs: None,
        log_namespace: Default::default(),
        use_otlp_decoding: use_otlp_decoding.into(),
    }
}

async fn send_and_collect_otel_event(
    use_otlp_decoding: bool,
    output_port: &str,
    endpoint: &str,
    body: Vec<u8>,
) -> Event {
    let (_guard_0, grpc_addr) = next_addr();
    let (_guard_1, http_addr) = next_addr();

    let source = get_source_config_with_headers(grpc_addr, http_addr, use_otlp_decoding);

    let (sender, output, _) = new_source(EventStatus::Delivered, output_port.to_string());
    let server = source
        .build(SourceContext::new_test(sender, None))
        .await
        .unwrap();
    tokio::spawn(server);
    test_util::wait_for_tcp(http_addr).await;

    let _res = reqwest::Client::new()
        .post(format!("http://{http_addr}/{endpoint}"))
        .header("Content-Type", "application/x-protobuf")
        .header("User-Agent", "Test")
        .body(body)
        .send()
        .await
        .expect("Failed to send request to OpenTelemetry source.");

    let mut events = test_util::collect_ready(output);
    assert_eq!(events.len(), 1);
    events.pop().unwrap()
}

#[tokio::test]
async fn http_headers_logs_use_otlp_decoding_false() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let (_guard_0, grpc_addr) = next_addr();
        let (_guard_1, http_addr) = next_addr();

        let source = get_source_config_with_headers(grpc_addr, http_addr, false);
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
        test_util::wait_for_tcp(http_addr).await;

        let client = reqwest::Client::new();
        let req = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: None,
                scope_logs: vec![ScopeLogs {
                    scope: None,
                    log_records: vec![LogRecord {
                        time_unix_nano: 1,
                        observed_time_unix_nano: 2,
                        severity_number: 9,
                        severity_text: "info".into(),
                        body: Some(AnyValue {
                            value: Some(StringValue("log body".into())),
                        }),
                        attributes: vec![],
                        dropped_attributes_count: 0,
                        flags: 4,
                        // opentelemetry sdk will hex::decode the given trace_id and span_id
                        trace_id: str_into_hex_bytes("4ac52aadf321c2e531db005df08792f5"),
                        span_id: str_into_hex_bytes("0b9e4bda2a55530d"),
                    }],
                    schema_url: "v1".into(),
                }],
                schema_url: "v1".into(),
            }],
        };
        let _res = client
            .post(format!("http://{http_addr}/v1/logs"))
            .header("Content-Type", "application/x-protobuf")
            .header("User-Agent", "Test")
            .body(req.encode_to_vec())
            .send()
            .await
            .expect("Failed to send log to Opentelemetry Collector.");

        let mut output = test_util::collect_ready(logs_output);
        assert_eq!(output.len(), 1);
        let actual_event = output.pop().unwrap();
        schema_definitions
            .unwrap()
            .assert_valid_for_event(&actual_event);
        let expect_vec = vec_into_btmap(vec![
            ("AbsentHeader", Value::Null),
            ("User-Agent", "Test".into()),
            ("message", "log body".into()),
            ("trace_id", "4ac52aadf321c2e531db005df08792f5".into()),
            ("span_id", "0b9e4bda2a55530d".into()),
            ("severity_number", 9.into()),
            ("severity_text", "info".into()),
            ("flags", 4.into()),
            ("dropped_attributes_count", 0.into()),
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

#[tokio::test]
async fn http_headers_logs_use_otlp_decoding_true() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let (_guard_0, grpc_addr) = next_addr();
        let (_guard_1, http_addr) = next_addr();

        let source = get_source_config_with_headers(grpc_addr, http_addr, true);

        let (sender, logs_output, _) = new_source(EventStatus::Delivered, LOGS.to_string());
        let server = source
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap();
        tokio::spawn(server);
        test_util::wait_for_tcp(http_addr).await;

        let client = reqwest::Client::new();
        let req = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: None,
                scope_logs: vec![ScopeLogs {
                    scope: None,
                    log_records: vec![LogRecord {
                        time_unix_nano: 1,
                        observed_time_unix_nano: 2,
                        severity_number: 9,
                        severity_text: "info".into(),
                        body: Some(AnyValue {
                            value: Some(StringValue("log body".into())),
                        }),
                        attributes: vec![],
                        dropped_attributes_count: 0,
                        flags: 4,
                        // opentelemetry sdk will hex::decode the given trace_id and span_id
                        trace_id: str_into_hex_bytes("4ac52aadf321c2e531db005df08792f5"),
                        span_id: str_into_hex_bytes("0b9e4bda2a55530d"),
                    }],
                    schema_url: "v1".into(),
                }],
                schema_url: "v1".into(),
            }],
        };
        let _res = client
            .post(format!("http://{http_addr}/v1/logs"))
            .header("Content-Type", "application/x-protobuf")
            .header("User-Agent", "Test")
            .body(req.encode_to_vec())
            .send()
            .await
            .expect("Failed to send log to Opentelemetry Collector.");

        let mut output = test_util::collect_ready(logs_output);
        assert_eq!(output.len(), 1);
        let actual_event = output.pop().unwrap();
        let log = actual_event.as_log();
        assert_eq!(log["AbsentHeader"], Value::Null);
        assert_eq!(log["User-Agent"], "Test".into());
    })
    .await;
}

#[tokio::test]
async fn http_headers_metrics_use_otlp_decoding_false() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let event = send_and_collect_otel_event(
            false,
            METRICS,
            "v1/metrics",
            create_test_metrics_request().encode_to_vec(),
        )
        .await;
        let metric = event.as_metric();
        assert_eq!(
            metric
                .metadata()
                .value()
                .get(path!("opentelemetry", "headers"))
                .unwrap()
                .get(path!("AbsentHeader"))
                .unwrap(),
            &Value::Null
        );
        assert_eq!(
            metric
                .metadata()
                .value()
                .get(path!("opentelemetry", "headers"))
                .unwrap()
                .get(path!("User-Agent"))
                .unwrap(),
            &value!("Test")
        );
    })
    .await;
}

#[tokio::test]
async fn http_headers_metrics_use_otlp_decoding_true() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let event = send_and_collect_otel_event(
            true,
            METRICS,
            "v1/metrics",
            create_test_metrics_request().encode_to_vec(),
        )
        .await;
        let log = event.as_log();
        assert_eq!(log["AbsentHeader"], Value::Null);
        assert_eq!(log["User-Agent"], "Test".into());
    })
    .await;
}

#[tokio::test]
async fn http_headers_traces_use_otlp_decoding_false() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let event = send_and_collect_otel_event(
            false,
            TRACES,
            "v1/traces",
            create_test_traces_request().encode_to_vec(),
        )
        .await;
        let trace = event.as_trace();
        assert_eq!(
            trace
                .metadata()
                .value()
                .get(path!("opentelemetry", "headers"))
                .unwrap()
                .get(path!("AbsentHeader"))
                .unwrap(),
            &Value::Null
        );
        assert_eq!(
            trace
                .metadata()
                .value()
                .get(path!("opentelemetry", "headers"))
                .unwrap()
                .get(path!("User-Agent"))
                .unwrap(),
            &value!("Test")
        );
        assert_eq!(
            event.metadata().trace_layout(),
            Some(TraceLayout::OtelFlattened)
        );
    })
    .await;
}

#[tokio::test]
async fn http_headers_traces_use_otlp_decoding_true() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let event = send_and_collect_otel_event(
            true,
            TRACES,
            "v1/traces",
            create_test_traces_request().encode_to_vec(),
        )
        .await;
        let trace = event.as_trace();
        assert_eq!(
            trace
                .metadata()
                .value()
                .get(path!("opentelemetry", "headers"))
                .unwrap()
                .get(path!("AbsentHeader"))
                .unwrap(),
            &Value::Null
        );
        assert_eq!(
            trace
                .metadata()
                .value()
                .get(path!("opentelemetry", "headers"))
                .unwrap()
                .get(path!("User-Agent"))
                .unwrap(),
            &value!("Test")
        );
        assert_eq!(
            event.metadata().trace_layout(),
            Some(TraceLayout::OtlpResourceSpans)
        );
    })
    .await;
}

async fn assert_grpc_trace_layout_marker(use_otlp_decoding: bool) {
    assert_source_compliance(&SOURCE_TAGS, async {
        let env = build_otlp_test_env_with(TRACES, None, use_otlp_decoding).await;
        let mut client = TraceServiceClient::connect(format!("http://{}", env.grpc_addr))
            .await
            .unwrap();
        _ = client
            .export(Request::new(create_test_traces_request()))
            .await;
        let mut events = test_util::collect_ready(env.output);
        assert_eq!(events.len(), 1);
        let expected = if use_otlp_decoding {
            TraceLayout::OtlpResourceSpans
        } else {
            TraceLayout::OtelFlattened
        };
        assert_eq!(
            events.pop().unwrap().metadata().trace_layout(),
            Some(expected)
        );
    })
    .await;
}

#[tokio::test]
async fn grpc_traces_use_otlp_decoding_false_sets_layout_marker() {
    assert_grpc_trace_layout_marker(false).await;
}

#[tokio::test]
async fn grpc_traces_use_otlp_decoding_true_sets_layout_marker() {
    assert_grpc_trace_layout_marker(true).await;
}

pub struct OTelTestEnv {
    pub grpc_addr: String,
    pub config: OpentelemetryConfig,
    pub output: Box<dyn Stream<Item = Event> + Unpin + Send>,
}

pub async fn build_otlp_test_env(
    event_name: &'static str,
    log_namespace: Option<bool>,
) -> OTelTestEnv {
    build_otlp_test_env_with(event_name, log_namespace, false).await
}

async fn build_otlp_test_env_with(
    event_name: &'static str,
    log_namespace: Option<bool>,
    use_otlp_decoding: bool,
) -> OTelTestEnv {
    let (_guard_0, grpc_addr) = next_addr();
    let (_guard_1, http_addr) = next_addr();

    let config = OpentelemetryConfig {
        grpc: GrpcConfig {
            address: grpc_addr,
            tls: Default::default(),
            keepalive: Default::default(),
        },
        http: HttpConfig {
            address: http_addr,
            tls: Default::default(),
            keepalive: Default::default(),
            headers: Default::default(),
        },
        acknowledgements: Default::default(),
        max_concurrent_requests: None,
        request_timeout_secs: None,
        log_namespace,
        use_otlp_decoding: use_otlp_decoding.into(),
    };

    let (sender, output, _) = new_source(EventStatus::Delivered, event_name.to_string());

    let server = config
        .build(SourceContext::new_test(sender.clone(), None))
        .await
        .expect("Failed to build source");

    tokio::spawn(server);
    test_util::wait_for_tcp(grpc_addr).await;

    OTelTestEnv {
        grpc_addr: grpc_addr.to_string(),
        config,
        output: Box::new(output),
    }
}

// Unlike `new_source`, receiving an event does not automatically finalize its acknowledgement.
fn new_unacknowledged_logs_source(
    config: &OpentelemetryConfig,
) -> (SourceSender, impl Stream<Item = Event> + Unpin) {
    let mut builder = SourceSender::builder();
    let logs_output = config
        .outputs(LogNamespace::Legacy)
        .into_iter()
        .find(|output| output.port.as_deref() == Some(LOGS))
        .unwrap();
    let output = builder
        .add_source_output(logs_output, "test".into())
        .into_stream()
        .flat_map(into_event_stream);
    (builder.build(), output)
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

fn current_time_and_nanos() -> (SystemTime, u64) {
    let time = SystemTime::now();
    let nanos = time
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() * 1_000_000_000 + u64::from(d.subsec_nanos()))
        .unwrap();
    (time, nanos)
}

#[tokio::test]
async fn http_logs_use_otlp_decoding_emits_metric() {
    use crate::metrics::Controller;

    test_util::trace_init();

    let (_guard_0, grpc_addr) = next_addr();
    let (_guard_1, http_addr) = next_addr();

    let source = OpentelemetryConfig {
        grpc: GrpcConfig {
            address: grpc_addr,
            tls: Default::default(),
            keepalive: Default::default(),
        },
        http: HttpConfig {
            address: http_addr,
            tls: Default::default(),
            keepalive: Default::default(),
            headers: Default::default(),
        },
        acknowledgements: Default::default(),
        max_concurrent_requests: None,
        request_timeout_secs: None,
        log_namespace: None,
        use_otlp_decoding: true.into(),
    };

    let (sender, logs_output, _) = new_source(EventStatus::Delivered, LOGS.to_string());
    let server = source
        .build(SourceContext::new_test(sender, None))
        .await
        .unwrap();
    tokio::spawn(server);
    test_util::wait_for_tcp(http_addr).await;

    let client = reqwest::Client::new();
    let req = ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: None,
            scope_logs: vec![ScopeLogs {
                scope: None,
                log_records: vec![LogRecord {
                    time_unix_nano: 1,
                    observed_time_unix_nano: 2,
                    severity_number: 9,
                    severity_text: "info".into(),
                    body: Some(AnyValue {
                        value: Some(StringValue("log body".into())),
                    }),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                    flags: 4,
                    trace_id: str_into_hex_bytes("4ac52aadf321c2e531db005df08792f5"),
                    span_id: str_into_hex_bytes("0b9e4bda2a55530d"),
                }],
                schema_url: "v1".into(),
            }],
            schema_url: "v1".into(),
        }],
    };
    let _res = client
        .post(format!("http://{http_addr}/v1/logs"))
        .header("Content-Type", "application/x-protobuf")
        .body(req.encode_to_vec())
        .send()
        .await
        .expect("Failed to send log to Opentelemetry Collector.");

    let mut output = test_util::collect_ready(logs_output);
    assert_eq!(output.len(), 1);
    output.pop().unwrap();

    // Check that the metric was emitted
    let metrics = Controller::get().unwrap().capture_metrics();
    let received_events_metric = metrics
        .iter()
        .find(|m| m.name() == "component_received_events_total")
        .expect("component_received_events_total metric should be present");

    // Verify it has a non-zero count
    match received_events_metric.value() {
        MetricValue::Counter { value } => {
            assert!(
                *value > 0.0,
                "component_received_events_total should be > 0, got {value}"
            );
        }
        _ => panic!("component_received_events_total should be a counter"),
    }
}

/// Sends a request to the OTLP/HTTP endpoint and returns the status code with the decoded
/// `google.rpc.Status` message from the response body.
async fn send_http_request(request: reqwest::RequestBuilder) -> (reqwest::StatusCode, String) {
    let response = request.send().await.expect("Failed to send request.");
    let status = response.status();
    let body = response
        .bytes()
        .await
        .expect("Failed to read response body.");
    let message = super::status::Status::decode(body)
        .expect("Response body should be a protobuf `Status`.")
        .message;
    (status, message)
}

#[tokio::test]
async fn http_rejections_return_client_error_status_codes() {
    let env = build_otlp_test_env(LOGS, None).await;
    let http_addr = env.config.http.address;
    test_util::wait_for_tcp(http_addr).await;
    let client = reqwest::Client::new();
    let logs_url = format!("http://{http_addr}/v1/logs");

    // JSON is not supported, so the request should be rejected as an unsupported media type.
    let (status, message) = send_http_request(
        client
            .post(&logs_url)
            .header("Content-Type", "application/json")
            .body(r#"{"resourceLogs":[]}"#),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(message.contains("application/x-protobuf"), "{message}");

    // A missing content type is treated the same as an unsupported one.
    let (status, _) = send_http_request(client.post(&logs_url)).await;
    assert_eq!(status, reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE);

    // Only `POST` is routed.
    let (status, _) = send_http_request(client.get(&logs_url)).await;
    assert_eq!(status, reqwest::StatusCode::METHOD_NOT_ALLOWED);

    // Unknown paths are not found.
    let (status, _) = send_http_request(
        client
            .post(format!("http://{http_addr}/v1/unknown"))
            .header("Content-Type", "application/x-protobuf"),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);

    // The supported content type is still accepted.
    let response = client
        .post(&logs_url)
        .header("Content-Type", "application/x-protobuf")
        .body(ExportLogsServiceRequest::default().encode_to_vec())
        .send()
        .await
        .expect("Failed to send request.");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

#[cfg(test)]
mod otlp_decoding_config_tests {
    use indoc::indoc;

    use crate::config::{DataType, LogNamespace, SourceConfig};
    use crate::sources::opentelemetry::config::{
        GrpcConfig, HttpConfig, OpentelemetryConfig, OtlpDecodingConfig,
    };
    use vector_lib::codecs::decoding::OtlpSignalType;

    #[test]
    fn test_otlp_decoding_mixed_configurations() {
        // Test single signal enabled
        let config = OtlpDecodingConfig {
            logs: false,
            metrics: false,
            traces: true,
        };
        assert!(config.any_enabled());
        assert!(!config.all_enabled());
        assert!(config.is_mixed());

        // Test two signals enabled
        let config = OtlpDecodingConfig {
            logs: true,
            metrics: false,
            traces: true,
        };
        assert!(config.any_enabled());
        assert!(!config.all_enabled());
        assert!(config.is_mixed());

        // Test different single signal
        let config = OtlpDecodingConfig {
            logs: true,
            metrics: false,
            traces: false,
        };
        assert!(config.any_enabled());
        assert!(!config.all_enabled());
        assert!(config.is_mixed());
    }

    #[test]
    fn test_otlp_decoding_from_bool() {
        // Test direct From<bool> trait implementation
        let config_true = OtlpDecodingConfig::from(true);
        assert!(config_true.logs);
        assert!(config_true.metrics);
        assert!(config_true.traces);
        assert!(config_true.all_enabled());
        assert!(!config_true.is_mixed());

        let config_false = OtlpDecodingConfig::from(false);
        assert!(!config_false.logs);
        assert!(!config_false.metrics);
        assert!(!config_false.traces);
        assert!(!config_false.any_enabled());
        assert!(!config_false.is_mixed());

        // Test YAML deserialization (which uses From<bool> under the hood)
        let config: OpentelemetryConfig = serde_yaml::from_str(indoc! {
            r#"
            use_otlp_decoding: true
            grpc:
              address: "0.0.0.0:4317"
            http:
              address: "0.0.0.0:4318"
            "#,
        })
        .unwrap();
        assert!(config.use_otlp_decoding.logs);
        assert!(config.use_otlp_decoding.metrics);
        assert!(config.use_otlp_decoding.traces);

        let config: OpentelemetryConfig = serde_yaml::from_str(indoc! {
            r#"
            use_otlp_decoding: false
            grpc:
              address: "0.0.0.0:4317"
            http:
              address: "0.0.0.0:4318"
            "#,
        })
        .unwrap();
        assert!(!config.use_otlp_decoding.logs);
        assert!(!config.use_otlp_decoding.metrics);
        assert!(!config.use_otlp_decoding.traces);
    }

    #[test]
    fn test_otlp_decoding_deserialization_from_struct() {
        // Test deserializing from a struct with all fields
        let config: OpentelemetryConfig = serde_yaml::from_str(indoc! {
            r#"
            grpc:
              address: "0.0.0.0:4317"
            http:
              address: "0.0.0.0:4318"
            use_otlp_decoding:
              logs: false
              metrics: false
              traces: true
            "#,
        })
        .unwrap();
        assert!(!config.use_otlp_decoding.logs);
        assert!(!config.use_otlp_decoding.metrics);
        assert!(config.use_otlp_decoding.traces);

        // Test deserializing from a struct with partial fields (using defaults)
        let config: OpentelemetryConfig = serde_yaml::from_str(indoc! {
            r#"
            grpc:
              address: "0.0.0.0:4317"
            http:
              address: "0.0.0.0:4318"
            use_otlp_decoding:
              traces: true
            "#,
        })
        .unwrap();
        assert!(!config.use_otlp_decoding.logs); // default false
        assert!(!config.use_otlp_decoding.metrics); // default false
        assert!(config.use_otlp_decoding.traces);
    }

    #[test]
    fn test_otlp_decoding_default_when_not_specified() {
        // Test that when use_otlp_decoding is not specified, it uses defaults (all false)
        let config: OpentelemetryConfig = serde_yaml::from_str(indoc! {
            r#"
            grpc:
              address: "0.0.0.0:4317"
            http:
              address: "0.0.0.0:4318"
            "#,
        })
        .unwrap();
        assert!(!config.use_otlp_decoding.logs);
        assert!(!config.use_otlp_decoding.metrics);
        assert!(!config.use_otlp_decoding.traces);
    }

    #[tokio::test]
    async fn test_get_signal_deserializer_per_signal() {
        let config_all_true = OpentelemetryConfig {
            grpc: GrpcConfig {
                address: "0.0.0.0:4317".parse().unwrap(),
                tls: None,
                keepalive: Default::default(),
            },
            http: HttpConfig {
                address: "0.0.0.0:4318".parse().unwrap(),
                tls: None,
                keepalive: Default::default(),
                headers: vec![],
            },
            acknowledgements: Default::default(),
            max_concurrent_requests: None,
            request_timeout_secs: None,
            log_namespace: None,
            use_otlp_decoding: OtlpDecodingConfig {
                logs: true,
                metrics: true,
                traces: true,
            },
        };

        // All should return Some deserializer
        assert!(
            config_all_true
                .get_signal_deserializer(OtlpSignalType::Logs)
                .unwrap()
                .is_some()
        );
        assert!(
            config_all_true
                .get_signal_deserializer(OtlpSignalType::Metrics)
                .unwrap()
                .is_some()
        );
        assert!(
            config_all_true
                .get_signal_deserializer(OtlpSignalType::Traces)
                .unwrap()
                .is_some()
        );

        let config_mixed = OpentelemetryConfig {
            grpc: GrpcConfig {
                address: "0.0.0.0:4317".parse().unwrap(),
                tls: None,
                keepalive: Default::default(),
            },
            http: HttpConfig {
                address: "0.0.0.0:4318".parse().unwrap(),
                tls: None,
                keepalive: Default::default(),
                headers: vec![],
            },
            acknowledgements: Default::default(),
            max_concurrent_requests: None,
            request_timeout_secs: None,
            log_namespace: None,
            use_otlp_decoding: OtlpDecodingConfig {
                logs: false,
                metrics: false,
                traces: true,
            },
        };

        // Only traces should return Some deserializer
        assert!(
            config_mixed
                .get_signal_deserializer(OtlpSignalType::Logs)
                .unwrap()
                .is_none()
        );
        assert!(
            config_mixed
                .get_signal_deserializer(OtlpSignalType::Metrics)
                .unwrap()
                .is_none()
        );
        assert!(
            config_mixed
                .get_signal_deserializer(OtlpSignalType::Traces)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn test_outputs_configuration_per_signal() {
        let config_mixed = OpentelemetryConfig {
            grpc: GrpcConfig {
                address: "0.0.0.0:4317".parse().unwrap(),
                tls: None,
                keepalive: Default::default(),
            },
            http: HttpConfig {
                address: "0.0.0.0:4318".parse().unwrap(),
                tls: None,
                keepalive: Default::default(),
                headers: vec![],
            },
            acknowledgements: Default::default(),
            max_concurrent_requests: None,
            request_timeout_secs: None,
            log_namespace: None,
            use_otlp_decoding: OtlpDecodingConfig {
                logs: false,
                metrics: true,
                traces: true,
            },
        };

        let outputs = config_mixed.outputs(LogNamespace::Legacy);
        assert_eq!(outputs.len(), 3);

        // Verify logs output (native format)
        let logs_output = &outputs[0];
        assert_eq!(logs_output.port.as_deref(), Some("logs"));
        assert_eq!(logs_output.ty, DataType::Log);

        // Verify metrics output (OTLP format, logs data type)
        let metrics_output = &outputs[1];
        assert_eq!(metrics_output.port.as_deref(), Some("metrics"));
        assert_eq!(metrics_output.ty, DataType::Log); // Should be Log when OTLP decoding is enabled

        // Verify traces output (OTLP format, traces data type)
        let traces_output = &outputs[2];
        assert_eq!(traces_output.port.as_deref(), Some("traces"));
        assert_eq!(traces_output.ty, DataType::Trace); // Should always be Trace regardless of OTLP decoding
    }
}
