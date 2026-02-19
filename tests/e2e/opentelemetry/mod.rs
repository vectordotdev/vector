pub mod logs;
pub mod metrics;

use std::{io, path::Path, process::Command};

use prost::Message as ProstMessage;
use prost_reflect::{DescriptorPool, prost::Message as ProstReflectMessage};
use vector_lib::opentelemetry::proto::{
    DESCRIPTOR_BYTES, common::v1::any_value::Value as AnyValueEnum, resource::v1::Resource,
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
    // Parse JSON and convert to VRL Value
    let vrl_value: VrlValue = serde_json::from_str::<serde_json::Value>(line)
        .map_err(|e| format!("Failed to parse JSON: {e}"))?
        .into();

    // Get the message descriptor from the descriptor pool
    let descriptor_pool = DescriptorPool::decode(DESCRIPTOR_BYTES)
        .map_err(|e| format!("Failed to decode descriptor pool: {e}"))?;

    let message_descriptor = descriptor_pool
        .get_message_by_name(request_message_type)
        .ok_or_else(|| {
            format!("Message type '{request_message_type}' not found in descriptor pool",)
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

    // Decode bytes into T (using prost 0.12.6)
    ProstMessage::decode(&buf[..])
        .map_err(|e| format!("Failed to decode ExportLogsServiceRequest: {e}"))
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
