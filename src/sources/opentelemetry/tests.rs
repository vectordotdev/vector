use crate::config::OutputId;
use crate::event::metric::{Bucket, Quantile};
use crate::event::{MetricKind, MetricTags, MetricValue};
use crate::{
    config::{SourceConfig, SourceContext},
    event::{
        into_event_stream, Event, EventStatus, LogEvent, Metric as MetricEvent, ObjectMap, Value,
    },
    sources::opentelemetry::{GrpcConfig, HttpConfig, OpentelemetryConfig, LOGS, METRICS},
    test_util::{
        self,
        components::{assert_source_compliance, SOURCE_TAGS},
        next_addr,
    },
    SourceSender,
};
use chrono::{DateTime, TimeZone, Utc};
use futures::Stream;
use futures_util::StreamExt;
use prost::Message;
use similar_asserts::assert_eq;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tonic::Request;
use vector_lib::config::LogNamespace;
use vector_lib::lookup::path;
use vector_lib::opentelemetry::proto::collector::metrics::v1::metrics_service_client::MetricsServiceClient;
use vector_lib::opentelemetry::proto::collector::metrics::v1::ExportMetricsServiceRequest;
use vector_lib::opentelemetry::proto::common::v1::any_value::Value::StringValue;
use vector_lib::opentelemetry::proto::metrics::v1::exponential_histogram_data_point::Buckets;
use vector_lib::opentelemetry::proto::metrics::v1::metric::Data;
use vector_lib::opentelemetry::proto::metrics::v1::summary_data_point::ValueAtQuantile;
use vector_lib::opentelemetry::proto::metrics::v1::{
    AggregationTemporality, ExponentialHistogram, ExponentialHistogramDataPoint, Gauge, Histogram,
    HistogramDataPoint, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum, Summary,
    SummaryDataPoint,
};
use vector_lib::opentelemetry::proto::resource::v1::Resource;
use vector_lib::opentelemetry::proto::{
    collector::logs::v1::{logs_service_client::LogsServiceClient, ExportLogsServiceRequest},
    common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue},
    logs::v1::{LogRecord, ResourceLogs, ScopeLogs},
    resource::v1::Resource as OtelResource,
};
use vrl::value;
use warp::http::HeaderMap;

#[test]
fn generate_config() {
    test_util::test_generate_config::<OpentelemetryConfig>();
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
        let req = Request::new(ExportLogsServiceRequest {
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
        });
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(env.output).await;
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
            .schema_definition(true);

        // send request via grpc client
        let mut client = LogsServiceClient::connect(format!("http://{}", env.grpc_addr))
            .await
            .unwrap();
        let req = Request::new(ExportLogsServiceRequest {
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
        });
        _ = client.export(req).await;
        let mut output = test_util::collect_ready(env.output).await;
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
        let mut output = test_util::collect_ready(env.output).await;
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
        let mut output = test_util::collect_ready(env.output).await;
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
        let mut output = test_util::collect_ready(env.output).await;
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
        let mut output = test_util::collect_ready(env.output).await;
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
        let mut output = test_util::collect_ready(env.output).await;
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
async fn receive_expontential_histogram_metric() {
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
        let mut output = test_util::collect_ready(env.output).await;
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
        let mut output = test_util::collect_ready(env.output).await;
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

#[tokio::test]
async fn http_headers() {
    assert_source_compliance(&SOURCE_TAGS, async {
        let grpc_addr = next_addr();
        let http_addr = next_addr();

        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", "test_client".parse().unwrap());
        headers.insert("Upgrade-Insecure-Requests", "false".parse().unwrap());
        headers.insert("X-Test-Header", "true".parse().unwrap());

        let source = OpentelemetryConfig {
            grpc: GrpcConfig {
                address: grpc_addr,
                tls: Default::default(),
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
                            value: Some(any_value::Value::StringValue("log body".into())),
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
            .post(format!("http://{}/v1/logs", http_addr))
            .header("Content-Type", "application/x-protobuf")
            .header("User-Agent", "Test")
            .body(req.encode_to_vec())
            .send()
            .await
            .expect("Failed to send log to Opentelemetry Collector.");

        let mut output = test_util::collect_ready(logs_output).await;
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

pub struct OTelTestEnv {
    pub grpc_addr: String,
    pub config: OpentelemetryConfig,
    pub output: Box<dyn Stream<Item = Event> + Unpin + Send>,
}

pub async fn build_otlp_test_env(
    event_name: &'static str,
    log_namespace: Option<bool>,
) -> OTelTestEnv {
    let grpc_addr = next_addr();
    let http_addr = next_addr();

    let config = OpentelemetryConfig {
        grpc: GrpcConfig {
            address: grpc_addr,
            tls: Default::default(),
        },
        http: HttpConfig {
            address: http_addr,
            tls: Default::default(),
            keepalive: Default::default(),
            headers: Default::default(),
        },
        acknowledgements: Default::default(),
        log_namespace,
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
