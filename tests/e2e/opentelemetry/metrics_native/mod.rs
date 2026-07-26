use crate::opentelemetry::{
    assert_service_name_with, parse_line_to_export_type_request, read_file_helper,
};

use vector_lib::opentelemetry::proto::{
    METRICS_REQUEST_MESSAGE_TYPE,
    collector::metrics::v1::ExportMetricsServiceRequest,
    common::v1::any_value::Value as AnyValueEnum,
    metrics::v1::{Gauge, Histogram, Sum, metric::Data as MetricData},
};

// telemetrygen emits 50 Gauge + 50 Sum + 50 Histogram. The source collector fans out to Vector
// over both gRPC and HTTP, so Vector sees each metric twice: 100 gauge + 100 sum + 100 histogram
// data points.
const EXPECTED_GAUGE: usize = 100;
const EXPECTED_SUM: usize = 100;
const EXPECTED_HISTOGRAM: usize = 100;
const EXPECTED_SCOPE_NAME: &str = "vector-e2e-metrics";
const EXPECTED_SCOPE_VERSION: &str = "1.2.3";

fn parse_export_metrics_request(content: &str) -> Result<ExportMetricsServiceRequest, String> {
    let mut merged = ExportMetricsServiceRequest {
        resource_metrics: Vec::new(),
    };

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        merged.resource_metrics.extend(
            parse_line_to_export_type_request::<ExportMetricsServiceRequest>(
                METRICS_REQUEST_MESSAGE_TYPE,
                line,
            )
            .map_err(|e| format!("Line {}: {}", line_num + 1, e))?
            .resource_metrics,
        );
    }

    if merged.resource_metrics.is_empty() {
        return Err("No resource metrics found in file".to_string());
    }
    Ok(merged)
}

/// Counts data points per (metric name, OTLP type) and validates each point:
/// non-empty name, non-zero timestamp, a value, and a round-tripped `metric.type` attribute.
fn tally_and_validate(request: &ExportMetricsServiceRequest) -> (usize, usize, usize) {
    let mut gauge = 0;
    let mut sum = 0;
    let mut histogram = 0;

    for rm in &request.resource_metrics {
        for sm in &rm.scope_metrics {
            let scope = sm.scope.as_ref().expect("scope_metrics has no scope");
            assert_eq!(scope.name, EXPECTED_SCOPE_NAME, "scope.name mismatch");
            assert_eq!(
                scope.version, EXPECTED_SCOPE_VERSION,
                "scope.version mismatch"
            );
            for metric in &sm.metrics {
                assert!(!metric.name.is_empty(), "metric name is empty");
                match metric.data.as_ref().expect("metric has no data") {
                    MetricData::Gauge(Gauge { data_points }) => {
                        assert_eq!(metric.name, "gauge_metric");
                        for dp in data_points {
                            assert!(dp.time_unix_nano > 0, "gauge point has zero timestamp");
                            assert!(dp.value.is_some(), "gauge point has no value");
                            assert_metric_type_attr(&dp.attributes, "gauge");
                        }
                        gauge += data_points.len();
                    }
                    MetricData::Sum(Sum { data_points, .. }) => {
                        assert_eq!(metric.name, "sum_metric");
                        for dp in data_points {
                            assert!(dp.time_unix_nano > 0, "sum point has zero timestamp");
                            assert!(dp.value.is_some(), "sum point has no value");
                            assert_metric_type_attr(&dp.attributes, "sum");
                        }
                        sum += data_points.len();
                    }
                    MetricData::Histogram(Histogram { data_points, .. }) => {
                        assert_eq!(metric.name, "histogram_metric");
                        for dp in data_points {
                            assert!(dp.time_unix_nano > 0, "histogram point has zero timestamp");
                            assert_metric_type_attr(&dp.attributes, "histogram");
                        }
                        histogram += data_points.len();
                    }
                    other => panic!("unexpected metric type for {}: {other:?}", metric.name),
                }
            }
        }
    }
    (gauge, sum, histogram)
}

fn assert_metric_type_attr(
    attributes: &[vector_lib::opentelemetry::proto::common::v1::KeyValue],
    expected: &str,
) {
    let attr = attributes
        .iter()
        .find(|kv| kv.key == "metric.type")
        .expect("missing 'metric.type' attribute");
    match attr.value.as_ref().and_then(|v| v.value.as_ref()) {
        Some(AnyValueEnum::StringValue(s)) => assert_eq!(s, expected),
        other => panic!("'metric.type' is not the expected string: {other:?}"),
    }
}

#[test]
fn vector_native_metrics_encode_to_otlp() {
    let content = read_file_helper("metrics-native", "collector-file-exporter.log")
        .expect("Failed to read collector file");
    let request = parse_export_metrics_request(&content)
        .expect("Failed to parse collector metrics as ExportMetricsServiceRequest");

    // service.name from the telemetrygen resource must survive the native round-trip.
    assert_service_name_with(
        &request.resource_metrics,
        "resource_metrics",
        "telemetrygen",
        |rm| rm.resource.as_ref(),
    );

    let (gauge, sum, histogram) = tally_and_validate(&request);
    assert_eq!(gauge, EXPECTED_GAUGE, "gauge_metric data point count");
    assert_eq!(sum, EXPECTED_SUM, "sum_metric data point count");
    assert_eq!(
        histogram, EXPECTED_HISTOGRAM,
        "histogram_metric data point count"
    );
}
