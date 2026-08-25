use chrono::{SubsecRound as _, Utc};
use vector_lib::{
    event::{EventStatus, Metric, MetricKind, MetricValue},
    metric_tags,
};

use super::*;
use crate::{
    SourceSender,
    config::{SinkConfig, SinkContext},
    sinks::{prometheus::remote_write::RemoteWriteConfig, util::HttpEndpoint},
    test_util::{self, wait_for_tcp},
    tls::MaybeTlsSettings,
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<PrometheusRemoteWriteConfig>();
}

#[tokio::test]
async fn receives_metrics_over_http() {
    receives_metrics(None).await;
}

#[tokio::test]
async fn receives_metrics_over_https() {
    receives_metrics(Some(TlsEnableableConfig::test_config())).await;
}

async fn receives_metrics(tls: Option<TlsEnableableConfig>) {
    let (_guard, address) = test_util::addr::next_addr();
    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let proto = MaybeTlsSettings::from_config(tls.as_ref(), true)
        .unwrap()
        .http_protocol_name();
    let source = PrometheusRemoteWriteConfig {
        address,
        path: default_path(),
        auth: None,
        tls: tls.clone(),
        metadata_conflict_strategy: Default::default(),
        acknowledgements: SourceAcknowledgementsConfig::default(),
        keepalive: KeepaliveConfig::default(),
        skip_nan_values: false,
    };
    let source = source
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;

    let sink = RemoteWriteConfig {
        endpoint: HttpEndpoint::parse(&format!("{}://localhost:{}/", proto, address.port()))
            .unwrap(),
        tls: tls.map(|tls| tls.options),
        ..Default::default()
    };
    let (sink, _) = sink
        .build(SinkContext::default())
        .await
        .expect("Error building config.");

    let events = make_events();
    let events_copy = events.clone();
    let mut output = test_util::spawn_collect_ready(
        async move {
            sink.run_events(events_copy).await.unwrap();
        },
        rx,
        1,
    )
    .await;

    // The MetricBuffer used by the sink may reorder the metrics, so
    // put them back into order before comparing.
    output.sort_unstable_by_key(|event| event.as_metric().name().to_owned());

    vector_lib::assert_event_data_eq!(events, output);
}

fn make_events() -> Vec<Event> {
    let timestamp = || Utc::now().trunc_subsecs(3);
    vec![
        Metric::new(
            "counter_1",
            MetricKind::Absolute,
            MetricValue::Counter { value: 42.0 },
        )
        .with_timestamp(Some(timestamp()))
        .into(),
        Metric::new(
            "gauge_2",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 41.0 },
        )
        .with_timestamp(Some(timestamp()))
        .into(),
        Metric::new(
            "histogram_3",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: vector_lib::buckets![ 2.3 => 11, 4.2 => 85 ],
                count: 96,
                sum: 156.2,
            },
        )
        .with_timestamp(Some(timestamp()))
        .into(),
        Metric::new(
            "summary_4",
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: vector_lib::quantiles![ 0.1 => 1.2, 0.5 => 3.6, 0.9 => 5.2 ],
                count: 23,
                sum: 8.6,
            },
        )
        .with_timestamp(Some(timestamp()))
        .into(),
    ]
}

async fn send_request_and_assert(port: u16, request_body: Vec<u8>) {
    // Send the request via HTTP POST
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://localhost:{}{}", port, default_path()))
        .header("Content-Type", "application/x-protobuf")
        .header("Content-Encoding", "snappy")
        .body(request_body)
        .send()
        .await
        .unwrap();

    // Should succeed (not return 400) despite conflicting metadata
    assert!(
        response.status().is_success(),
        "Expected success but got: {}",
        response.status()
    );
}

fn create_default_request_body() -> Vec<u8> {
    use prost::Message;
    use vector_lib::prometheus::parser::proto;

    let request = proto::WriteRequest {
        metadata: vec![proto::MetricMetadata {
            r#type: proto::MetricType::Gauge as i32,
            metric_family_name: "test_metric".into(),
            help: "Gauge definition".into(),
            unit: String::default(),
        }],
        timeseries: vec![proto::TimeSeries {
            labels: vec![proto::Label {
                name: "__name__".into(),
                value: "test_metric".into(),
            }],
            samples: vec![proto::Sample {
                value: 42.0,
                timestamp: chrono::Utc::now().timestamp_millis(),
            }],
        }],
    };

    let mut buf = Vec::new();
    request.encode(&mut buf).unwrap();

    // Compress with snappy as expected by the remote_write endpoint
    snap::raw::Encoder::new().compress_vec(&buf).unwrap()
}

fn create_conflicting_metadata_request_body() -> Vec<u8> {
    use prost::Message;
    use vector_lib::prometheus::parser::proto;

    let request = proto::WriteRequest {
        metadata: vec![
            proto::MetricMetadata {
                r#type: proto::MetricType::Gauge as i32,
                metric_family_name: "test_metric".into(),
                help: "First definition as gauge".into(),
                unit: String::default(),
            },
            proto::MetricMetadata {
                r#type: proto::MetricType::Counter as i32,
                metric_family_name: "test_metric".into(),
                help: "Conflicting definition as counter".into(),
                unit: String::default(),
            },
        ],
        timeseries: vec![proto::TimeSeries {
            labels: vec![proto::Label {
                name: "__name__".into(),
                value: "test_metric".into(),
            }],
            samples: vec![proto::Sample {
                value: 42.0,
                timestamp: chrono::Utc::now().timestamp_millis(),
            }],
        }],
    };

    let mut buf = Vec::new();
    request.encode(&mut buf).unwrap();

    // Compress with snappy as expected by the remote_write endpoint
    snap::raw::Encoder::new().compress_vec(&buf).unwrap()
}

async fn send_request(port: u16, request_body: Vec<u8>) -> reqwest::Response {
    let client = reqwest::Client::new();
    client
        .post(format!("http://localhost:{}{}", port, default_path()))
        .header("Content-Type", "application/x-protobuf")
        .header("Content-Encoding", "snappy")
        .body(request_body)
        .send()
        .await
        .unwrap()
}

/// According to the [spec](https://github.com/OpenObservability/OpenMetrics/blob/main/specification/OpenMetrics.md?plain=1#L115)
/// > Label names MUST be unique within a LabelSet.
/// Prometheus itself will reject the metric with an error. Largely to remain backward compatible with older versions of Vector,
/// we accept the metric, but take the last label in the list.
#[tokio::test]
async fn receives_metrics_duplicate_labels() {
    let (_guard, address) = test_util::addr::next_addr();
    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let source = PrometheusRemoteWriteConfig {
        address,
        path: default_path(),
        auth: None,
        tls: None,
        metadata_conflict_strategy: Default::default(),
        acknowledgements: SourceAcknowledgementsConfig::default(),
        keepalive: KeepaliveConfig::default(),
        skip_nan_values: false,
    };
    let source = source
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;

    let sink = RemoteWriteConfig {
        endpoint: HttpEndpoint::parse(&format!("http://localhost:{}/", address.port())).unwrap(),
        ..Default::default()
    };
    let (sink, _) = sink
        .build(SinkContext::default())
        .await
        .expect("Error building config.");

    let timestamp = Utc::now().trunc_subsecs(3);

    let events = vec![
        Metric::new(
            "gauge_2",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 41.0 },
        )
        .with_timestamp(Some(timestamp))
        .with_tags(Some(metric_tags! {
            "code" => "200".to_string(),
            "code" => "success".to_string(),
        }))
        .into(),
    ];

    let expected = vec![
        Metric::new(
            "gauge_2",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 41.0 },
        )
        .with_timestamp(Some(timestamp))
        .with_tags(Some(metric_tags! {
            "code" => "success".to_string(),
        }))
        .into(),
    ];

    let output = test_util::spawn_collect_ready(
        async move {
            sink.run_events(events).await.unwrap();
        },
        rx,
        1,
    )
    .await;

    vector_lib::assert_event_data_eq!(expected, output);
}

#[tokio::test]
async fn test_skip_nan_values_enabled() {
    let (_guard, address) = test_util::addr::next_addr();
    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let source = PrometheusRemoteWriteConfig {
        address,
        path: default_path(),
        auth: None,
        tls: None,
        metadata_conflict_strategy: Default::default(),
        acknowledgements: SourceAcknowledgementsConfig::default(),
        keepalive: KeepaliveConfig::default(),
        skip_nan_values: true,
    };
    let source = source
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;

    // Create a request with NaN values
    let request_body = {
        use prost::Message;
        use vector_lib::prometheus::parser::proto;

        let request = proto::WriteRequest {
            metadata: vec![],
            timeseries: vec![
                proto::TimeSeries {
                    labels: vec![proto::Label {
                        name: "__name__".into(),
                        value: "test_metric_valid".into(),
                    }],
                    samples: vec![proto::Sample {
                        value: 42.0,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    }],
                },
                proto::TimeSeries {
                    labels: vec![proto::Label {
                        name: "__name__".into(),
                        value: "test_metric_nan".into(),
                    }],
                    samples: vec![proto::Sample {
                        value: f64::NAN,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    }],
                },
            ],
        };

        let mut buf = Vec::new();
        request.encode(&mut buf).unwrap();

        // Compress with snappy as expected by the remote_write endpoint
        snap::raw::Encoder::new().compress_vec(&buf).unwrap()
    };

    send_request_and_assert(address.port(), request_body).await;

    // Verify we only received the valid metric (NaN metric should be filtered)
    let output = test_util::collect_ready(rx).await;
    assert_eq!(output.len(), 1);

    let metric = output[0].as_metric();
    assert_eq!(metric.name(), "test_metric_valid");
    assert_eq!(metric.value(), &MetricValue::Gauge { value: 42.0 });
}

#[tokio::test]
async fn test_skip_nan_values_disabled() {
    let (_guard, address) = test_util::addr::next_addr();
    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let source = PrometheusRemoteWriteConfig {
        address,
        path: default_path(),
        auth: None,
        tls: None,
        metadata_conflict_strategy: Default::default(),
        acknowledgements: SourceAcknowledgementsConfig::default(),
        keepalive: KeepaliveConfig::default(),
        skip_nan_values: false,
    };
    let source = source
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;

    // Create a request with NaN values
    let request_body = {
        use prost::Message;
        use vector_lib::prometheus::parser::proto;

        let request = proto::WriteRequest {
            metadata: vec![],
            timeseries: vec![
                proto::TimeSeries {
                    labels: vec![proto::Label {
                        name: "__name__".into(),
                        value: "test_metric_valid".into(),
                    }],
                    samples: vec![proto::Sample {
                        value: 42.0,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    }],
                },
                proto::TimeSeries {
                    labels: vec![proto::Label {
                        name: "__name__".into(),
                        value: "test_metric_nan".into(),
                    }],
                    samples: vec![proto::Sample {
                        value: f64::NAN,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    }],
                },
            ],
        };

        let mut buf = Vec::new();
        request.encode(&mut buf).unwrap();

        // Compress with snappy as expected by the remote_write endpoint
        snap::raw::Encoder::new().compress_vec(&buf).unwrap()
    };

    send_request_and_assert(address.port(), request_body).await;

    // Verify we received both metrics (including NaN metric)
    let mut output = test_util::collect_ready(rx).await;
    assert_eq!(output.len(), 2);

    // Sort by name for predictable testing
    output.sort_by(|a, b| a.as_metric().name().cmp(b.as_metric().name()));

    // Check the NaN metric
    let nan_metric = output[0].as_metric();
    assert_eq!(nan_metric.name(), "test_metric_nan");
    match nan_metric.value() {
        MetricValue::Gauge { value } => {
            assert!(value.is_nan());
        }
        _ => panic!("Expected gauge metric"),
    }

    // Check the valid metric
    let valid_metric = output[1].as_metric();
    assert_eq!(valid_metric.name(), "test_metric_valid");
    assert_eq!(valid_metric.value(), &MetricValue::Gauge { value: 42.0 });
}

#[tokio::test]
async fn receives_metrics_on_custom_path() {
    let (_guard, address) = test_util::addr::next_addr();
    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let source = PrometheusRemoteWriteConfig {
        address,
        path: "/api/v1/write".to_string(),
        auth: None,
        tls: None,
        metadata_conflict_strategy: Default::default(),
        acknowledgements: SourceAcknowledgementsConfig::default(),
        keepalive: KeepaliveConfig::default(),
        skip_nan_values: false,
    };
    let source = source
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;

    let sink = RemoteWriteConfig {
        endpoint: HttpEndpoint::parse(&format!("http://localhost:{}/api/v1/write", address.port()))
            .unwrap(),
        ..Default::default()
    };
    let (sink, _) = sink
        .build(SinkContext::default())
        .await
        .expect("Error building config.");

    let events = make_events();
    let events_copy = events.clone();
    let mut output = test_util::spawn_collect_ready(
        async move {
            sink.run_events(events_copy).await.unwrap();
        },
        rx,
        1,
    )
    .await;

    // The MetricBuffer used by the sink may reorder the metrics, so
    // put them back into order before comparing.
    output.sort_unstable_by_key(|event| event.as_metric().name().to_owned());

    vector_lib::assert_event_data_eq!(events, output);
}

#[tokio::test]
async fn rejects_metrics_on_wrong_path() {
    let (_guard, address) = test_util::addr::next_addr();
    let (tx, _rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let source = PrometheusRemoteWriteConfig {
        address,
        path: "/api/v1/write".to_string(),
        auth: None,
        tls: None,
        metadata_conflict_strategy: Default::default(),
        acknowledgements: SourceAcknowledgementsConfig::default(),
        keepalive: KeepaliveConfig::default(),
        skip_nan_values: false,
    };
    let source = source
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;

    // Try to send to the root path, which should be rejected
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://localhost:{}/wrong/path", address.port()))
        .header("Content-Type", "application/x-protobuf")
        .body(vec![])
        .send()
        .await
        .unwrap();

    // Should return an error status code since we're sending to the wrong path
    assert!(
        response.status().is_client_error(),
        "Expected 4xx error, got {}",
        response.status()
    );
}

#[tokio::test]
async fn receives_metrics_on_default_path() {
    let (_guard, address) = test_util::addr::next_addr();
    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let source = PrometheusRemoteWriteConfig {
        address,
        path: default_path(),
        auth: None,
        tls: None,
        metadata_conflict_strategy: Default::default(),
        acknowledgements: SourceAcknowledgementsConfig::default(),
        keepalive: KeepaliveConfig::default(),
        skip_nan_values: false,
    };
    let source = source
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;

    let request_body = create_default_request_body();
    send_request_and_assert(address.port(), request_body).await;

    // Verify we received the metric data
    let output = test_util::collect_ready(rx).await;
    assert_eq!(output.len(), 1);

    let metric = output[0].as_metric();
    assert_eq!(metric.name(), "test_metric");
    assert_eq!(metric.value(), &MetricValue::Gauge { value: 42.0 });
}

#[tokio::test]
async fn rejects_metrics_on_wrong_path_with_skip_nan_enabled() {
    let (_guard, address) = test_util::addr::next_addr();
    let (tx, _rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let source = PrometheusRemoteWriteConfig {
        address,
        path: "/api/v1/write".to_string(),
        auth: None,
        tls: None,
        metadata_conflict_strategy: Default::default(),
        acknowledgements: SourceAcknowledgementsConfig::default(),
        keepalive: KeepaliveConfig::default(),
        skip_nan_values: true,
    };
    let source = source
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;

    // Try to send to the root path, which should be rejected
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://localhost:{}/wrong/path", address.port()))
        .header("Content-Type", "application/x-protobuf")
        .body(vec![])
        .send()
        .await
        .unwrap();

    // Should return an error status code since we're sending to the wrong path
    assert!(
        response.status().is_client_error(),
        "Expected 4xx error, got {}",
        response.status()
    );
}

#[tokio::test]
async fn accepts_conflicting_metadata() {
    let (_guard, address) = test_util::addr::next_addr();
    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let source = PrometheusRemoteWriteConfig {
        address,
        path: default_path(),
        auth: None,
        tls: None,
        metadata_conflict_strategy: MetadataConflictStrategy::Ignore,
        acknowledgements: SourceAcknowledgementsConfig::default(),
        keepalive: KeepaliveConfig::default(),
        skip_nan_values: false,
    };
    let source = source
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;

    let request_body = create_conflicting_metadata_request_body();
    let response = send_request(address.port(), request_body).await;

    // Should succeed (not return 400) despite conflicting metadata
    assert!(
        response.status().is_success(),
        "Expected success but got: {}",
        response.status()
    );

    // Verify we received the metric data
    let output = test_util::collect_ready(rx).await;
    assert_eq!(output.len(), 1);

    let metric = output[0].as_metric();
    assert_eq!(metric.name(), "test_metric");
    assert_eq!(metric.value(), &MetricValue::Gauge { value: 42.0 });
}

#[tokio::test]
async fn rejects_conflicting_metadata() {
    let (_guard, address) = test_util::addr::next_addr();
    let (tx, _rx) = SourceSender::new_test_finalize(EventStatus::Delivered);

    let source = PrometheusRemoteWriteConfig {
        address,
        path: default_path(),
        auth: None,
        tls: None,
        metadata_conflict_strategy: MetadataConflictStrategy::Reject,
        acknowledgements: SourceAcknowledgementsConfig::default(),
        keepalive: KeepaliveConfig::default(),
        skip_nan_values: false,
    };
    let source = source
        .build(SourceContext::new_test(tx, None))
        .await
        .unwrap();
    tokio::spawn(source);
    wait_for_tcp(address).await;

    let request_body = create_conflicting_metadata_request_body();
    let response = send_request(address.port(), request_body).await;

    // Should be rejected
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
