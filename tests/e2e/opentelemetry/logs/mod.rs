use prost::Message as ProstMessage;
use prost_reflect::{DescriptorPool, prost::Message as ProstReflectMessage};
use serde_json::Value as JsonValue;
use std::{io, path::Path, process::Command};
use vector_lib::opentelemetry::proto::collector::logs::v1::ExportLogsServiceRequest;
use vector_lib::opentelemetry::proto::common::v1::any_value::Value as AnyValueEnum;
use vector_lib::opentelemetry::proto::{DESCRIPTOR_BYTES, LOGS_REQUEST_MESSAGE_TYPE};
use vrl::value::Value as VrlValue;

const EXPECTED_LOG_COUNT: usize = 100;
const EXPECTED_VECTOR_LOG_COUNT: usize = 200; // 100 via gRPC + 100 via HTTP

fn read_file_helper(filename: &str) -> Result<String, io::Error> {
    let local_path = Path::new("/output/opentelemetry-logs").join(filename);
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
                "opentelemetry-logs_vector_target:/output",
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

fn parse_line_to_export_logs_request(line: &str) -> Result<ExportLogsServiceRequest, String> {
    // Parse JSON and convert to VRL Value
    let vrl_value: VrlValue = serde_json::from_str::<JsonValue>(line)
        .map_err(|e| format!("Failed to parse JSON: {e}"))?
        .into();

    // Get the message descriptor from the descriptor pool
    let descriptor_pool = DescriptorPool::decode(DESCRIPTOR_BYTES)
        .map_err(|e| format!("Failed to decode descriptor pool: {e}"))?;

    let message_descriptor = descriptor_pool
        .get_message_by_name(LOGS_REQUEST_MESSAGE_TYPE)
        .ok_or_else(|| {
            format!("Message type '{LOGS_REQUEST_MESSAGE_TYPE}' not found in descriptor pool",)
        })?;

    // Encode VRL Value to DynamicMessage using VRL's encode_message with JSON names enabled
    let dynamic_message = vrl::protobuf::encode::encode_message(
        &message_descriptor,
        vrl_value,
        &vrl::protobuf::encode::Options {
            use_json_names: true,
        },
    )
    .map_err(|e| format!("Failed to encode VRL value to protobuf: {e}"))?;

    // Encode DynamicMessage to bytes (using prost 0.13.5)
    let mut buf = Vec::new();
    ProstReflectMessage::encode(&dynamic_message, &mut buf)
        .map_err(|e| format!("Failed to encode dynamic message to bytes: {e}"))?;

    // Decode bytes into ExportLogsServiceRequest (using prost 0.12.6)
    ProstMessage::decode(&buf[..])
        .map_err(|e| format!("Failed to decode ExportLogsServiceRequest: {e}"))
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
            parse_line_to_export_logs_request(line)
                .map_err(|e| format!("Line {}: {}", line_num + 1, e))?
                .resource_logs,
        );
    }

    if merged_request.resource_logs.is_empty() {
        return Err("No resource logs found in file".to_string());
    }

    Ok(merged_request)
}

/// Asserts that all resource logs have a `service.name` attribute set to `"telemetrygen"`.
fn assert_service_name(request: &ExportLogsServiceRequest) {
    for (i, rl) in request.resource_logs.iter().enumerate() {
        let resource = rl
            .resource
            .as_ref()
            .unwrap_or_else(|| panic!("resource_logs[{i}] missing resource"));

        let service_name_attr = resource
            .attributes
            .iter()
            .find(|kv| kv.key == "service.name")
            .unwrap_or_else(|| panic!("resource_logs[{i}] missing 'service.name' attribute"));

        let actual_value = service_name_attr
            .value
            .as_ref()
            .and_then(|v| v.value.as_ref())
            .unwrap_or_else(|| panic!("resource_logs[{i}] 'service.name' has no value"));

        if let AnyValueEnum::StringValue(s) = actual_value {
            assert_eq!(
                s, "telemetrygen",
                "resource_logs[{i}] 'service.name' expected 'telemetrygen', got '{s}'"
            );
        } else {
            panic!("resource_logs[{i}] 'service.name' is not a string value");
        }
    }
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
    let collector_content =
        read_file_helper("collector-file-exporter.log").expect("Failed to read collector file");
    let vector_content =
        read_file_helper("vector-file-sink.log").expect("Failed to read vector file");

    let collector_request = parse_export_logs_request(&collector_content)
        .expect("Failed to parse collector logs as ExportLogsServiceRequest");
    let vector_request = parse_export_logs_request(&vector_content)
        .expect("Failed to parse vector logs as ExportLogsServiceRequest");

    // Count total log records in collector output
    let collector_log_count = collector_request
        .resource_logs
        .iter()
        .flat_map(|rl| &rl.scope_logs)
        .flat_map(|sl| &sl.log_records)
        .count();

    // Count total log records in vector output
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
        vector_log_count, EXPECTED_VECTOR_LOG_COUNT,
        "Vector produced {vector_log_count} log records, expected {EXPECTED_VECTOR_LOG_COUNT}"
    );

    // Verify service.name attribute
    assert_service_name(&collector_request);
    assert_service_name(&vector_request);

    // Verify static log record fields
    assert_log_records_static_fields(&collector_request);
    assert_log_records_static_fields(&vector_request);

    // Note: We don't compare collector_request == vector_request because Vector receives
    // logs from both gRPC and HTTP exporters (200 logs vs 100 from collector file exporter).
    // The assertions above verify that all logs have correct structure and content.
}
