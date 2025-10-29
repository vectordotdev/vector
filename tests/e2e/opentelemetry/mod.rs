pub mod logs;
pub mod metrics;

use std::{io, path::Path, process::Command};

use prost::Message as ProstMessage;
use prost_reflect::{DescriptorPool, prost::Message as ProstReflectMessage};
use vector_lib::opentelemetry::proto::DESCRIPTOR_BYTES;
use vrl::value::Value as VrlValue;

fn read_file_helper(test_type: &str, filename: &str) -> Result<String, io::Error> {
    let local_path = Path::new(format!("/output/opentelemetry-{test_type}")).join(filename);
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
                &format!("opentelemetry-{test_type}_vector_target:/output"),
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
