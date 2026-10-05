pub mod demo;
pub mod logs;
pub mod metrics;
pub mod metrics_native;
pub mod traces;

use std::{io, path::Path, process::Command};

use base64::prelude::{BASE64_STANDARD, Engine as _};
use prost::Message as ProstMessage;
use prost_reflect::{
    DescriptorPool, Kind, MessageDescriptor, prost::Message as ProstReflectMessage,
};
use vector_lib::opentelemetry::proto::{
    DESCRIPTOR_BYTES, TRACES_REQUEST_MESSAGE_TYPE, collector::trace::v1::ExportTraceServiceRequest,
    common::v1::any_value::Value as AnyValueEnum, resource::v1::Resource,
};
use vrl::value::Value as VrlValue;

fn read_file_helper(data_type: &str, filename: &str) -> Result<String, io::Error> {
    let local_path = Path::new(&format!("/output/opentelemetry-{data_type}")).join(filename);
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
                &format!("opentelemetry-{data_type}_vector_target:/output"),
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

fn parse_line_to_export_type_request<Message>(
    request_message_type: &str,
    line: &str,
) -> Result<Message, String>
where
    Message: ProstMessage + Default,
{
    let vrl_value: VrlValue = serde_json::from_str::<serde_json::Value>(line)
        .map_err(|e| format!("Failed to parse JSON: {e}"))?
        .into();

    parse_value_to_export_type_request(request_message_type, vrl_value)
}

fn parse_value_to_export_type_request<Message>(
    request_message_type: &str,
    vrl_value: VrlValue,
) -> Result<Message, String>
where
    Message: ProstMessage + Default,
{
    let descriptor_pool = DescriptorPool::decode(DESCRIPTOR_BYTES)
        .map_err(|e| format!("Failed to decode descriptor pool: {e}"))?;

    let message_descriptor = descriptor_pool
        .get_message_by_name(request_message_type)
        .ok_or_else(|| {
            format!("Message type '{request_message_type}' not found in descriptor pool",)
        })?;

    let dynamic_message = vrl::protobuf::encode::encode_message(
        &message_descriptor,
        vrl_value,
        &vrl::protobuf::encode::Options {
            use_json_names: true,
            allow_lossy_string_coercion: true,
        },
    )
    .map_err(|e| format!("Failed to encode VRL value to protobuf: {e}"))?;

    let mut buf = Vec::new();
    ProstReflectMessage::encode(&dynamic_message, &mut buf)
        .map_err(|e| format!("Failed to encode dynamic message to bytes: {e}"))?;

    ProstMessage::decode(&buf[..]).map_err(|e| format!("Failed to decode protobuf message: {e}"))
}

pub fn assert_service_name_with<ResourceT, F>(
    request: &[ResourceT],
    resource_name: &str,
    expected_name: &str,
    get_resource: F,
) where
    F: Fn(&ResourceT) -> Option<&Resource>,
{
    for (i, item) in request.iter().enumerate() {
        let resource =
            get_resource(item).unwrap_or_else(|| panic!("{resource_name}[{i}] missing resource"));
        let service_name_attr = resource
            .attributes
            .iter()
            .find(|kv| kv.key == "service.name")
            .unwrap_or_else(|| panic!("{resource_name}[{i}] missing 'service.name' attribute"));
        let actual_value = service_name_attr
            .value
            .as_ref()
            .and_then(|v| v.value.as_ref())
            .unwrap_or_else(|| panic!("{resource_name}[{i}] 'service.name' has no value"));
        if let AnyValueEnum::StringValue(s) = actual_value {
            assert_eq!(
                s, expected_name,
                "{resource_name}[{i}] 'service.name' expected '{expected_name}', got '{s}'"
            );
        } else {
            panic!("{resource_name}[{i}] 'service.name' is not a string value");
        }
    }
}

/// Verifies that the component_received_events_total internal metric counts
/// individual log records/metrics/spans, not batch requests.
/// This ensures consistency when use_otlp_decoding is enabled.
pub fn assert_component_received_events_total(data_type: &str, expected_count: usize) {
    let metrics_content = read_file_helper(data_type, "vector-internal-metrics-sink.log")
        .expect("Failed to read internal metrics file");

    // Parse the metrics file to find component_received_events_total
    let mut found_metric = false;
    let mut total_events = 0u64;

    for line in metrics_content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse the JSON metric
        let metric: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("Failed to parse metrics JSON: {e}"));

        // Look for component_received_events_total metric
        if let Some(name) = metric.get("name").and_then(|v| v.as_str())
            && name == "component_received_events_total"
        {
            // Check if this is for our opentelemetry source
            if let Some(tags) = metric.get("tags")
                && let Some(component_id) = tags.get("component_id").and_then(|v| v.as_str())
                && component_id == "source0"
            {
                found_metric = true;
                // Get the counter value
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
        "Could not find component_received_events_total metric for source0 in internal metrics"
    );

    // Verify that the metric counts individual items, not batch requests
    assert_eq!(
        total_events, expected_count as u64,
        "component_received_events_total should count individual items ({expected_count}), \
         not batch requests. Found: {total_events}"
    );
}

pub(super) fn parse_export_traces_request(
    content: &str,
) -> Result<ExportTraceServiceRequest, String> {
    // The file may contain multiple lines, each with a JSON object containing an array of resourceSpans
    let mut merged_request = ExportTraceServiceRequest {
        resource_spans: Vec::new(),
    };

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        merged_request.resource_spans.extend(
            parse_collector_trace_line(line)
                .map_err(|e| format!("Line {}: {}", line_num + 1, e))?
                .resource_spans,
        );
    }

    if merged_request.resource_spans.is_empty() {
        return Err("No resource spans found in file".to_string());
    }

    Ok(merged_request)
}

fn parse_collector_trace_line(line: &str) -> Result<ExportTraceServiceRequest, String> {
    let mut value: VrlValue = serde_json::from_str::<serde_json::Value>(line)
        .map_err(|e| format!("Failed to parse JSON: {e}"))?
        .into();

    let pool = DescriptorPool::decode(DESCRIPTOR_BYTES).map_err(|error| error.to_string())?;
    let descriptor = pool
        .get_message_by_name(TRACES_REQUEST_MESSAGE_TYPE)
        .ok_or("Trace request descriptor missing")?;
    reject_unknown_fields(&value, &descriptor)?;
    decode_collector_ids(&mut value)?;
    parse_value_to_export_type_request(TRACES_REQUEST_MESSAGE_TYPE, value)
}

// VRL's protobuf encoder otherwise discards unknown JSON fields from both captures.
fn reject_unknown_fields(value: &VrlValue, descriptor: &MessageDescriptor) -> Result<(), String> {
    let fields = value
        .as_object()
        .ok_or_else(|| format!("{} should be an object", descriptor.full_name()))?;
    for (name, value) in fields {
        let field = descriptor.get_field_by_json_name(name).ok_or_else(|| {
            format!(
                "Unsupported captured field {}.{name}",
                descriptor.full_name()
            )
        })?;
        if let Kind::Message(child) = field.kind() {
            if field.is_list() {
                for value in value
                    .as_array()
                    .ok_or_else(|| format!("{name} should be an array"))?
                {
                    reject_unknown_fields(value, &child)?;
                }
            } else if !matches!(value, VrlValue::Null) {
                reject_unknown_fields(value, &child)?;
            }
        }
    }
    Ok(())
}

fn decode_collector_ids(value: &mut VrlValue) -> Result<(), String> {
    match value {
        VrlValue::Object(fields) => {
            for (name, value) in fields {
                if name.as_str() == "bytesValue" {
                    let encoded = value
                        .as_bytes()
                        .ok_or("bytesValue should be a base64 string")?;
                    *value = VrlValue::Bytes(
                        BASE64_STANDARD
                            .decode(encoded)
                            .map_err(|error| format!("Failed to decode bytesValue: {error}"))?
                            .into(),
                    );
                    continue;
                }
                let valid_hex_lengths: &[usize] = match name.as_str() {
                    "traceId" => &[32],
                    "spanId" => &[16],
                    "parentSpanId" => &[0, 16],
                    _ => {
                        decode_collector_ids(value)?;
                        continue;
                    }
                };

                let encoded = value
                    .as_bytes()
                    .ok_or_else(|| format!("{name} should be a hexadecimal string"))?;
                if !valid_hex_lengths.contains(&encoded.len()) {
                    return Err(format!(
                        "{name} has invalid hexadecimal length {}",
                        encoded.len()
                    ));
                }

                *value = VrlValue::Bytes(
                    hex::decode(encoded.as_ref())
                        .map_err(|e| format!("Failed to decode {name}: {e}"))?
                        .into(),
                );
            }
        }
        VrlValue::Array(values) => {
            for value in values {
                decode_collector_ids(value)?;
            }
        }
        _ => {}
    }

    Ok(())
}

#[test]
fn collector_trace_parser_rejects_unknown_fields() {
    let capture = r#"{"resourceSpans":[{"resource":{"entityRefs":[]},"scopeSpans":[]}]}"#;
    assert!(
        parse_export_traces_request(capture)
            .unwrap_err()
            .contains("entityRefs")
    );
}

#[test]
fn collector_trace_parser_decodes_byte_attributes() {
    let capture = r#"{"resourceSpans":[{"scopeSpans":[{"spans":[{"attributes":[{"key":"payload","value":{"bytesValue":"AAEC/w=="}}]}]}]}]}"#;
    let request = parse_export_traces_request(capture).unwrap();
    assert_eq!(
        request.resource_spans[0].scope_spans[0].spans[0].attributes[0].value,
        Some(vector_lib::opentelemetry::proto::common::v1::AnyValue {
            value: Some(AnyValueEnum::BytesValue(vec![0, 1, 2, 255]))
        })
    );
}
