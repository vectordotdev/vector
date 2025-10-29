use crate::opentelemetry::{parse_line_to_export_type_request, read_file_helper};

use vector_lib::opentelemetry::proto::METRICS_REQUEST_MESSAGE_TYPE;
use vector_lib::opentelemetry::proto::collector::metrics::v1::ExportMetricsServiceRequest;
use vector_lib::opentelemetry::proto::common::v1::any_value::Value as AnyValueEnum;

const EXPECTED_METRIC_COUNT: usize = 200; // 100 via gRPC + 100 via HTTP

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

/// Asserts that all resource metrics have a `service.name` attribute set to `"telemetrygen"`.
fn assert_service_name(request: &ExportMetricsServiceRequest) {
    for (i, rl) in request.resource_metrics.iter().enumerate() {
        let resource = rl
            .resource
            .as_ref()
            .unwrap_or_else(|| panic!("resource_metrics[{i}] missing resource"));

        let service_name_attr = resource
            .attributes
            .iter()
            .find(|kv| kv.key == "service.name")
            .unwrap_or_else(|| panic!("resource_metrics[{i}] missing 'service.name' attribute"));

        let actual_value = service_name_attr
            .value
            .as_ref()
            .and_then(|v| v.value.as_ref())
            .unwrap_or_else(|| panic!("resource_metrics[{i}] 'service.name' has no value"));

        if let AnyValueEnum::StringValue(s) = actual_value {
            assert_eq!(
                s, "telemetrygen",
                "resource_metrics[{i}] 'service.name' expected 'telemetrygen', got '{s}'"
            );
        } else {
            panic!("resource_metrics[{i}] 'service.name' is not a string value");
        }
    }
}

/// Asserts that all metric records have static field values:
/// - `body`: `"the message"`
/// - `severityText`: `"Info"`
fn assert_metric_records_static_fields(request: &ExportMetricsServiceRequest) {
    for (rl_idx, rl) in request.resource_metrics.iter().enumerate() {
        for (sl_idx, sl) in rl.scope_metrics.iter().enumerate() {
            for (lr_idx, metric) in sl.metrics.iter().enumerate() {
                let prefix =
                    format!("resource_metrics[{rl_idx}].scope_metrics[{sl_idx}].metrics[{lr_idx}]");

                // Assert body is "the message"
                let body_value = metric
                    .body
                    .as_ref()
                    .unwrap_or_else(|| panic!("{prefix} missing body"))
                    .value
                    .as_ref()
                    .unwrap_or_else(|| panic!("{prefix} body has no value"));

                if let AnyValueEnum::StringValue(s) = body_value {
                    assert_eq!(
                        s, "the message",
                        "{prefix} body expected 'the message', got '{s}'"
                    );
                } else {
                    panic!("{prefix} body is not a string value");
                }

                // Assert severityText is "Info"
                assert_eq!(
                    metric.severity_text, "Info",
                    "{prefix} severityText expected 'Info', got '{}'",
                    metric.severity_text
                );
                // timeUnixNano is ignored as it varies
            }
        }
    }
}

#[test]
fn vector_sink_otel_sink_metrics_match() {
    let collector_content = read_file_helper("metrics", "collector-file-exporter.log")
        .expect("Failed to read collector file");
    let vector_content =
        read_file_helper("metrics", "vector-file-sink.log").expect("Failed to read vector file");

    let collector_request = parse_export_metrics_request(&collector_content)
        .expect("Failed to parse collector metrics as ExportLogsServiceRequest");
    let vector_request = parse_export_metrics_request(&vector_content)
        .expect("Failed to parse vector metrics as ExportLogsServiceRequest");

    // Count total log records
    let collector_metric_count = collector_request
        .resource_metrics
        .iter()
        .flat_map(|rl| &rl.scope_metrics)
        .flat_map(|sl| &sl.metrics)
        .count();

    let vector_metric_count = vector_request
        .resource_metrics
        .iter()
        .flat_map(|rl| &rl.scope_metrics)
        .flat_map(|sl| &sl.metrics)
        .count();

    assert_eq!(
        collector_metric_count, EXPECTED_METRIC_COUNT,
        "Collector produced {collector_metric_count} metrics, expected {EXPECTED_METRIC_COUNT}"
    );

    assert_eq!(
        vector_metric_count, EXPECTED_METRIC_COUNT,
        "Vector produced {vector_metric_count} metrics, expected {EXPECTED_METRIC_COUNT}"
    );

    // Verify service.name attribute
    assert_service_name(&collector_request);
    assert_service_name(&vector_request);

    // Verify static log record fields
    assert_metric_records_static_fields(&collector_request);
    assert_metric_records_static_fields(&vector_request);

    // Both collector and Vector receive 200 metrics total (100 via gRPC + 100 via HTTP).
    // Compare them directly to verify the entire pipeline works correctly.
    assert_eq!(
        collector_request, vector_request,
        "Collector and Vector log requests should match"
    );
}
