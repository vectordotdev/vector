use vector_lib::opentelemetry::proto::{
    LOGS_REQUEST_MESSAGE_TYPE, collector::logs::v1::ExportLogsServiceRequest,
    common::v1::any_value::Value as AnyValueEnum,
};

use crate::opentelemetry::{
    assert_service_name_with, parse_line_to_export_type_request, read_file_helper,
};

const EXPECTED_LOG_COUNT: usize = 200; // 100 via gRPC + 100 via HTTP

fn parse_export_logs_request(content: &str) -> Result<ExportLogsServiceRequest, String> {
    // The file may contain multiple lines, each with a JSON object containing an array of resourceLogs
    let mut merged_request = ExportLogsServiceRequest {
        resource_logs: Vec::new(),
    };

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Merge resource_logs from this request into the accumulated result
        merged_request.resource_logs.extend(
            parse_line_to_export_type_request::<ExportLogsServiceRequest>(
                LOGS_REQUEST_MESSAGE_TYPE,
                line,
            )
            .map_err(|e| format!("Line {}: {}", line_num + 1, e))?
            .resource_logs,
        );
    }

    if merged_request.resource_logs.is_empty() {
        return Err("No resource logs found in file".to_string());
    }

    Ok(merged_request)
}

/// Asserts that all log records have static field values:
/// - `body`: `"the message"`
/// - `severityText`: `"Info"`
fn assert_log_records_static_fields(request: &ExportLogsServiceRequest) {
    for (rl_idx, rl) in request.resource_logs.iter().enumerate() {
        for (sl_idx, sl) in rl.scope_logs.iter().enumerate() {
            for (lr_idx, log_record) in sl.log_records.iter().enumerate() {
                let prefix =
                    format!("resource_logs[{rl_idx}].scope_logs[{sl_idx}].log_records[{lr_idx}]");

                // Assert body is "the message"
                let body_value = log_record
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
                    log_record.severity_text, "Info",
                    "{prefix} severityText expected 'Info', got '{}'",
                    log_record.severity_text
                );
                // timeUnixNano is ignored as it varies
            }
        }
    }
}

#[test]
fn vector_sink_otel_sink_logs_match() {
    let collector_content = read_file_helper("logs", "collector-file-exporter.log")
        .expect("Failed to read collector file");
    let vector_content =
        read_file_helper("logs", "vector-file-sink.log").expect("Failed to read vector file");

    let collector_request = parse_export_logs_request(&collector_content)
        .expect("Failed to parse collector logs as ExportLogsServiceRequest");
    let vector_request = parse_export_logs_request(&vector_content)
        .expect("Failed to parse vector logs as ExportLogsServiceRequest");

    // Count total log records
    let collector_log_count = collector_request
        .resource_logs
        .iter()
        .flat_map(|rl| &rl.scope_logs)
        .flat_map(|sl| &sl.log_records)
        .count();

    let vector_log_count = vector_request
        .resource_logs
        .iter()
        .flat_map(|rl| &rl.scope_logs)
        .flat_map(|sl| &sl.log_records)
        .count();

    assert_eq!(
        collector_log_count, EXPECTED_LOG_COUNT,
        "Collector produced {collector_log_count} log records, expected {EXPECTED_LOG_COUNT}"
    );

    assert_eq!(
        vector_log_count, EXPECTED_LOG_COUNT,
        "Vector produced {vector_log_count} log records, expected {EXPECTED_LOG_COUNT}"
    );

    // Verify service.name attribute
    assert_service_name_with(
        &collector_request.resource_logs,
        "resource_logs",
        "telemetrygen",
        |rl| rl.resource.as_ref(),
    );
    assert_service_name_with(
        &vector_request.resource_logs,
        "resource_logs",
        "telemetrygen",
        |rl| rl.resource.as_ref(),
    );

    // Verify static log record fields
    assert_log_records_static_fields(&collector_request);
    assert_log_records_static_fields(&vector_request);

    // Both collector and Vector receive 200 logs total (100 via gRPC + 100 via HTTP).
    // Compare them directly to verify the entire pipeline works correctly.
    assert_eq!(
        collector_request, vector_request,
        "Collector and Vector log requests should match"
    );
}
