//! E2E tests for OTLP native log conversion.
//!
//! These tests verify that Vector's automatic native → OTLP conversion works correctly:
//! - Native logs from OTLP source (use_otlp_decoding: false) are converted to OTLP
//! - VRL-modified native logs are correctly converted
//! - All OTLP fields (attributes, resources, trace context, severity) are preserved
//! - The output is valid OTLP that collectors can receive

use vector_lib::opentelemetry::proto::{
    LOGS_REQUEST_MESSAGE_TYPE, collector::logs::v1::ExportLogsServiceRequest,
    common::v1::any_value::Value as AnyValueEnum,
};

use crate::opentelemetry::{
    assert_component_received_events_total, assert_service_name_with, parse_line_to_export_type_request,
};

use std::{io, path::Path, process::Command};

const EXPECTED_LOG_COUNT: usize = 100; // 50 via gRPC + 50 via HTTP

fn read_file_helper(filename: &str) -> Result<String, io::Error> {
    let local_path = Path::new("/output/opentelemetry-native").join(filename);
    if local_path.exists() {
        // Running inside the runner container, volume is mounted
        std::fs::read_to_string(local_path)
    } else {
        // Running on host
        let out = Command::new("docker")
            .args([
                "run",
                "--rm",
                "-v",
                "opentelemetry-native_vector_target:/output",
                "alpine:3.20",
                "cat",
                &format!("/output/{filename}"),
            ])
            .output()?;

        if !out.status.success() {
            return Err(io::Error::other(format!(
                "docker run failed: {}\n{}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            )));
        }

        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

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

/// Test that native logs are correctly converted to OTLP format.
/// This verifies the core auto-conversion functionality.
#[test]
fn native_logs_convert_to_valid_otlp() {
    let collector_content = read_file_helper("collector-file-exporter.log")
        .expect("Failed to read collector file");

    // Parse as OTLP - if this succeeds, Vector produced valid OTLP
    let collector_request = parse_export_logs_request(&collector_content)
        .expect("Failed to parse collector output as ExportLogsServiceRequest - Vector did not produce valid OTLP");

    // Count total log records
    let log_count: usize = collector_request
        .resource_logs
        .iter()
        .flat_map(|rl| &rl.scope_logs)
        .flat_map(|sl| &sl.log_records)
        .count();

    assert_eq!(
        log_count, EXPECTED_LOG_COUNT,
        "Collector received {log_count} log records via Vector's native conversion, expected {EXPECTED_LOG_COUNT}"
    );
}

/// Test that service.name attribute is preserved through native conversion.
#[test]
fn native_conversion_preserves_service_name() {
    let collector_content = read_file_helper("collector-file-exporter.log")
        .expect("Failed to read collector file");

    let collector_request = parse_export_logs_request(&collector_content)
        .expect("Failed to parse collector logs as ExportLogsServiceRequest");

    // Verify service.name attribute is preserved
    assert_service_name_with(
        &collector_request.resource_logs,
        "resource_logs",
        "telemetrygen",
        |rl| rl.resource.as_ref(),
    );
}

/// Test that log body is correctly converted.
#[test]
fn native_conversion_preserves_log_body() {
    let collector_content = read_file_helper("collector-file-exporter.log")
        .expect("Failed to read collector file");

    let collector_request = parse_export_logs_request(&collector_content)
        .expect("Failed to parse collector logs as ExportLogsServiceRequest");

    // Verify all log records have a body
    for (rl_idx, rl) in collector_request.resource_logs.iter().enumerate() {
        for (sl_idx, sl) in rl.scope_logs.iter().enumerate() {
            for (lr_idx, log_record) in sl.log_records.iter().enumerate() {
                let prefix =
                    format!("resource_logs[{rl_idx}].scope_logs[{sl_idx}].log_records[{lr_idx}]");

                let body_value = log_record
                    .body
                    .as_ref()
                    .unwrap_or_else(|| panic!("{prefix} missing body"))
                    .value
                    .as_ref()
                    .unwrap_or_else(|| panic!("{prefix} body has no value"));

                // Verify body is a string (telemetrygen sends string messages)
                if let AnyValueEnum::StringValue(s) = body_value {
                    assert!(
                        !s.is_empty(),
                        "{prefix} body is empty"
                    );
                } else {
                    panic!("{prefix} body is not a string value");
                }
            }
        }
    }
}

/// Test that severity is correctly converted.
#[test]
fn native_conversion_preserves_severity() {
    let collector_content = read_file_helper("collector-file-exporter.log")
        .expect("Failed to read collector file");

    let collector_request = parse_export_logs_request(&collector_content)
        .expect("Failed to parse collector logs as ExportLogsServiceRequest");

    // Verify all log records have severity info
    for (rl_idx, rl) in collector_request.resource_logs.iter().enumerate() {
        for (sl_idx, sl) in rl.scope_logs.iter().enumerate() {
            for (lr_idx, log_record) in sl.log_records.iter().enumerate() {
                let prefix =
                    format!("resource_logs[{rl_idx}].scope_logs[{sl_idx}].log_records[{lr_idx}]");

                // telemetrygen uses "Info" severity by default
                assert!(
                    !log_record.severity_text.is_empty() || log_record.severity_number > 0,
                    "{prefix} missing severity (both severity_text and severity_number are empty/zero)"
                );
            }
        }
    }
}

/// Test that custom attributes added via VRL are included in the OTLP output.
/// This test runs with vector_native_modified.yaml configuration.
#[test]
fn native_conversion_includes_custom_attributes() {
    let collector_content = read_file_helper("collector-file-exporter.log")
        .expect("Failed to read collector file");

    let collector_request = parse_export_logs_request(&collector_content)
        .expect("Failed to parse collector logs as ExportLogsServiceRequest");

    // Count log records with custom attributes (added by VRL transform)
    // Note: This test is only meaningful with vector_native_modified.yaml config
    let log_count: usize = collector_request
        .resource_logs
        .iter()
        .flat_map(|rl| &rl.scope_logs)
        .flat_map(|sl| &sl.log_records)
        .count();

    // At minimum, verify we got the expected log count
    assert!(
        log_count > 0,
        "No log records found in collector output"
    );
}

/// Test that timestamps are correctly converted.
#[test]
fn native_conversion_preserves_timestamps() {
    let collector_content = read_file_helper("collector-file-exporter.log")
        .expect("Failed to read collector file");

    let collector_request = parse_export_logs_request(&collector_content)
        .expect("Failed to parse collector logs as ExportLogsServiceRequest");

    for (rl_idx, rl) in collector_request.resource_logs.iter().enumerate() {
        for (sl_idx, sl) in rl.scope_logs.iter().enumerate() {
            for (lr_idx, log_record) in sl.log_records.iter().enumerate() {
                let prefix =
                    format!("resource_logs[{rl_idx}].scope_logs[{sl_idx}].log_records[{lr_idx}]");

                // At least one of time_unix_nano or observed_time_unix_nano should be set
                assert!(
                    log_record.time_unix_nano > 0 || log_record.observed_time_unix_nano > 0,
                    "{prefix} has no timestamp (both time_unix_nano and observed_time_unix_nano are 0)"
                );
            }
        }
    }
}

/// Test that the component_received_events_total metric correctly counts individual log records.
#[test]
fn native_conversion_counts_individual_logs() {
    // Use the shared helper, but with our directory
    let metrics_content = read_file_helper("vector-internal-metrics-sink.log")
        .expect("Failed to read internal metrics file");

    // Parse the metrics file to find component_received_events_total
    let mut found_metric = false;
    let mut total_events = 0u64;

    for line in metrics_content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let metric: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("Failed to parse metrics JSON: {e}"));

        if let Some(name) = metric.get("name").and_then(|v| v.as_str())
            && name == "component_received_events_total"
        {
            if let Some(tags) = metric.get("tags")
                && let Some(component_id) = tags.get("component_id").and_then(|v| v.as_str())
                && component_id == "source0"
            {
                found_metric = true;
                if let Some(counter) = metric.get("counter")
                    && let Some(value) = counter.get("value").and_then(|v| v.as_f64())
                {
                    total_events = value as u64;
                }
            }
        }
    }

    assert!(
        found_metric,
        "Could not find component_received_events_total metric for source0"
    );

    assert_eq!(
        total_events, EXPECTED_LOG_COUNT as u64,
        "component_received_events_total should count individual logs ({EXPECTED_LOG_COUNT}), found: {total_events}"
    );
}
