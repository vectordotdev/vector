use crate::opentelemetry::{
    assert_service_name_with, parse_line_to_export_type_request, read_file_helper,
};

use vector_lib::opentelemetry::proto::{
    METRICS_REQUEST_MESSAGE_TYPE,
    collector::metrics::v1::ExportMetricsServiceRequest,
    common::v1::{KeyValue, any_value::Value as AnyValueEnum},
    metrics::v1::{Gauge, Sum, metric::Data as MetricData},
};

const EXPECTED_METRIC_COUNT: usize = 400; // 200 via gRPC + 200 via HTTP (50 of each type: Gauge, Sum, Histogram, ExponentialHistogram)

fn parse_export_metrics_request(content: &str) -> Result<ExportMetricsServiceRequest, String> {
    // The file may contain multiple lines, each with a JSON object containing an array of
    // resourceMetrics
    let mut merged_request = ExportMetricsServiceRequest {
        resource_metrics: Vec::new(),
    };

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Merge resource_metrics from this request into the accumulated result
        merged_request.resource_metrics.extend(
            parse_line_to_export_type_request::<ExportMetricsServiceRequest>(
                METRICS_REQUEST_MESSAGE_TYPE,
                line,
            )
            .map_err(|e| format!("Line {}: {}", line_num + 1, e))?
            .resource_metrics,
        );
    }

    if merged_request.resource_metrics.is_empty() {
        return Err("No resource metrics found in file".to_string());
    }

    Ok(merged_request)
}

/// Asserts that all metrics have:
/// - A non-empty name
/// - At least one data point
/// - Each data point has a valid timestamp and value
fn assert_metric_data_points(request: &ExportMetricsServiceRequest) {
    for (rm_idx, rm) in request.resource_metrics.iter().enumerate() {
        for (sm_idx, sm) in rm.scope_metrics.iter().enumerate() {
            for (m_idx, metric) in sm.metrics.iter().enumerate() {
                let prefix =
                    format!("resource_metrics[{rm_idx}].scope_metrics[{sm_idx}].metrics[{m_idx}]");

                // Assert metric has a name
                assert!(!metric.name.is_empty(), "{prefix} metric name is empty");

                // Get data points based on metric type
                let data_points_count = match &metric
                    .data
                    .as_ref()
                    .unwrap_or_else(|| panic!("{prefix} has no data"))
                {
                    MetricData::Gauge(Gauge { data_points, .. })
                    | MetricData::Sum(Sum { data_points, .. }) => {
                        assert!(!data_points.is_empty(), "{prefix} has no data points");
                        for (dp_idx, dp) in data_points.iter().enumerate() {
                            assert!(
                                dp.time_unix_nano > 0,
                                "{prefix}.gauge.data_points[{dp_idx}] has invalid timestamp"
                            );
                            assert!(
                                dp.value.is_some(),
                                "{prefix}.gauge.data_points[{dp_idx}] has no value"
                            );
                        }
                        data_points.len()
                    }
                    MetricData::Histogram(histogram) => {
                        assert!(
                            !histogram.data_points.is_empty(),
                            "{prefix} histogram has no data points"
                        );
                        histogram.data_points.len()
                    }
                    MetricData::ExponentialHistogram(exp_histogram) => {
                        assert!(
                            !exp_histogram.data_points.is_empty(),
                            "{prefix} exponential histogram has no data points"
                        );
                        exp_histogram.data_points.len()
                    }
                    // not supported by telemetrygen
                    MetricData::Summary(_) => panic!("Unexpected Summary metric"),
                };

                assert!(data_points_count > 0, "{prefix} has zero data points");
            }
        }
    }
}

/// Asserts that each metric has the expected telemetry attribute "metric.type"
fn assert_metric_attributes(request: &ExportMetricsServiceRequest) {
    for (rm_idx, rm) in request.resource_metrics.iter().enumerate() {
        for (sm_idx, sm) in rm.scope_metrics.iter().enumerate() {
            for (m_idx, metric) in sm.metrics.iter().enumerate() {
                let prefix =
                    format!("resource_metrics[{rm_idx}].scope_metrics[{sm_idx}].metrics[{m_idx}]");

                // Get data points and verify attributes
                let attrs: Box<dyn Iterator<Item = &Vec<KeyValue>>> = match metric
                    .data
                    .as_ref()
                    .unwrap_or_else(|| panic!("{prefix} has no data"))
                {
                    MetricData::Gauge(g) => {
                        assert_eq!(metric.name.as_str(), "gauge_metric");
                        Box::new(g.data_points.iter().map(|g| &g.attributes))
                    }
                    MetricData::Sum(s) => {
                        assert_eq!(metric.name.as_str(), "sum_metric");
                        Box::new(s.data_points.iter().map(|s| &s.attributes))
                    }
                    MetricData::Histogram(h) => {
                        assert_eq!(metric.name.as_str(), "histogram_metric");
                        Box::new(h.data_points.iter().map(|h| &h.attributes))
                    }
                    MetricData::ExponentialHistogram(h) => {
                        assert_eq!(metric.name.as_str(), "exponential_histogram_metric");
                        Box::new(h.data_points.iter().map(|h| &h.attributes))
                    }
                    // not supported by telemetrygen
                    MetricData::Summary(_) => panic!("Unexpected Summary metric"),
                };
                let expected_attr_value = metric.name.strip_suffix("_metric").unwrap();

                // Verify gauge and sum data point attributes
                for (idx, attributes) in attrs.enumerate() {
                    let attr = attributes
                        .iter()
                        .find(|kv| kv.key == "metric.type")
                        .unwrap_or_else(|| {
                            panic!("{prefix}.data_points[{idx}] missing 'metric.type' attribute")
                        });

                    if let Some(AnyValueEnum::StringValue(s)) =
                        attr.value.as_ref().and_then(|v| v.value.as_ref())
                    {
                        assert_eq!(
                            s, expected_attr_value,
                            "{prefix}.data_points[{idx}] 'metric.type' expected '{expected_attr_value}', got '{s}'"
                        );
                    } else {
                        panic!("{prefix}.data_points[{idx}] 'metric.type' is not a string value");
                    }
                }
            }
        }
    }
}

/// Asserts that metrics have the expected names and counts by type
/// Expected: 100 gauge_metric (50 gRPC + 50 HTTP), 100 sum_metric, 100 histogram_metric, 100 exponential_histogram_metric
fn assert_metric_names_and_types(request: &ExportMetricsServiceRequest) {
    use std::collections::HashMap;

    let mut metric_type_counts: HashMap<(&str, &str), usize> = HashMap::new();

    for rm in &request.resource_metrics {
        for sm in &rm.scope_metrics {
            for metric in &sm.metrics {
                let type_name = match &metric.data {
                    Some(MetricData::Gauge(_)) => "Gauge",
                    Some(MetricData::Sum(_)) => "Sum",
                    Some(MetricData::Histogram(_)) => "Histogram",
                    Some(MetricData::ExponentialHistogram(_)) => "ExponentialHistogram",
                    Some(MetricData::Summary(_)) | None => panic!("unexpected MetricData type"),
                };

                *metric_type_counts
                    .entry((&metric.name, type_name))
                    .or_insert(0) += 1;
            }
        }
    }

    // Verify we have exactly 100 of each metric type with the correct name
    // (50 via gRPC + 50 via HTTP = 100 total per type)
    assert_eq!(
        metric_type_counts.get(&("gauge_metric", "Gauge")),
        Some(&100),
        "Expected 100 gauge_metric (Gauge), got {:?}",
        metric_type_counts.get(&("gauge_metric", "Gauge"))
    );

    assert_eq!(
        metric_type_counts.get(&("sum_metric", "Sum")),
        Some(&100),
        "Expected 100 sum_metric (Sum), got {:?}",
        metric_type_counts.get(&("sum_metric", "Sum"))
    );

    assert_eq!(
        metric_type_counts.get(&("histogram_metric", "Histogram")),
        Some(&100),
        "Expected 100 histogram_metric (Histogram), got {:?}",
        metric_type_counts.get(&("histogram_metric", "Histogram"))
    );

    assert_eq!(
        metric_type_counts.get(&("exponential_histogram_metric", "ExponentialHistogram")),
        Some(&100),
        "Expected 100 exponential_histogram_metric (ExponentialHistogram), got {:?}",
        metric_type_counts.get(&("exponential_histogram_metric", "ExponentialHistogram"))
    );

    // Verify total count
    let total_count: usize = metric_type_counts.values().sum();
    assert_eq!(
        total_count, EXPECTED_METRIC_COUNT,
        "Total metric count mismatch. Breakdown: {:?}",
        metric_type_counts
    );
}

#[test]
fn vector_sink_otel_sink_metrics_match() {
    let collector_content = read_file_helper("metrics", "collector-file-exporter.log")
        .expect("Failed to read collector file");
    let vector_content =
        read_file_helper("metrics", "vector-file-sink.log").expect("Failed to read vector file");

    let collector_request = parse_export_metrics_request(&collector_content)
        .expect("Failed to parse collector metrics as ExportMetricsServiceRequest");
    let vector_request = parse_export_metrics_request(&vector_content)
        .expect("Failed to parse vector metrics as ExportMetricsServiceRequest");

    // Count total data points across all metric types
    let count_data_points = |request: &ExportMetricsServiceRequest| -> usize {
        request
            .resource_metrics
            .iter()
            .flat_map(|rm| &rm.scope_metrics)
            .flat_map(|sm| &sm.metrics)
            .map(|m| match &m.data {
                Some(MetricData::Gauge(g)) => g.data_points.len(),
                Some(MetricData::Sum(s)) => s.data_points.len(),
                Some(MetricData::Histogram(h)) => h.data_points.len(),
                Some(MetricData::ExponentialHistogram(eh)) => eh.data_points.len(),
                Some(MetricData::Summary(_)) => panic!("Unexpected Summary metric"),
                None => 0,
            })
            .sum()
    };

    let collector_metric_count = count_data_points(&collector_request);
    let vector_metric_count = count_data_points(&vector_request);

    assert_eq!(
        collector_metric_count, EXPECTED_METRIC_COUNT,
        "Collector produced {collector_metric_count} metric data points, expected {EXPECTED_METRIC_COUNT}"
    );

    assert_eq!(
        vector_metric_count, EXPECTED_METRIC_COUNT,
        "Vector produced {vector_metric_count} metric data points, expected {EXPECTED_METRIC_COUNT}"
    );

    // Verify service.name attribute
    assert_service_name_with(
        &collector_request.resource_metrics,
        "resource_metrics",
        "telemetrygen",
        |rl| rl.resource.as_ref(),
    );
    assert_service_name_with(
        &vector_request.resource_metrics,
        "resource_metrics",
        "telemetrygen",
        |rl| rl.resource.as_ref(),
    );

    // Verify metric data points are valid
    assert_metric_data_points(&collector_request);
    assert_metric_data_points(&vector_request);

    // Verify metric names and types match expectations
    assert_metric_names_and_types(&collector_request);
    assert_metric_names_and_types(&vector_request);

    // Verify metric attributes are correct
    assert_metric_attributes(&collector_request);
    assert_metric_attributes(&vector_request);

    // Both collector and Vector receive 400 metrics total (200 via gRPC + 200 via HTTP).
    // The 200 metrics consist of 50 each of: Gauge, Sum, Histogram, and ExponentialHistogram.
    // Compare them directly to verify the entire pipeline works correctly.
    assert_eq!(
        collector_request, vector_request,
        "Collector and Vector metric requests should match"
    );
}
