use serde_json::Value;
use std::{collections::BTreeMap, io, path::Path, process::Command};

const EXPECTED_LOG_COUNT: usize = 100;

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
                "vector_target:/output",
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

fn extract_timestamp(value: &Value) -> u64 {
    value
        .get("timeUnixNano")
        .and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| v.as_u64())
        })
        .expect("Missing or invalid 'timeUnixNano' in log record")
}

fn sanitize(mut value: Value) -> Value {
    if let Some(obj) = value.as_object_mut() {
        obj.remove("traceId");
        obj.remove("spanId");
    };
    value
}

fn normalize_numbers_to_strings(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let normalized = map
                .iter()
                // Ignore severityNumber field because Vector's json codec outputs the numeric value ("9")
                // while the collector's file exporter outputs the protobuf enum name ("SEVERITY_NUMBER_INFO").
                // Both formats are valid; we compare severityText instead which matches on both sides.
                .filter(|(k, _)| k.as_str() != "severityNumber")
                // Ignore empty attributes arrays - some encoders omit the field when empty, others include it.
                .filter(|(k, v)| {
                    if k.as_str() == "attributes" {
                        !matches!(v, Value::Array(arr) if arr.is_empty())
                    } else {
                        true
                    }
                })
                .map(|(k, v)| (k.clone(), normalize_numbers_to_strings(v)))
                .collect();
            Value::Object(normalized)
        }
        Value::Array(arr) => {
            let normalized = arr.iter().map(normalize_numbers_to_strings).collect();
            Value::Array(normalized)
        }
        Value::Number(n) => Value::String(n.to_string()),
        other => other.clone(),
    }
}

fn parse_log_records(content: String) -> BTreeMap<u64, Value> {
    let mut result = BTreeMap::new();

    for (idx, line) in content.lines().enumerate() {
        let root: Value = serde_json::from_str(line)
            .unwrap_or_else(|_| panic!("Line {idx} is malformed: {line}"));

        let resource_logs = root
            .get("resourceLogs")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("Missing or invalid 'resourceLogs' in line {idx}"));

        for resource in resource_logs {
            let scope_logs = resource
                .get("scopeLogs")
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| panic!("Missing or invalid 'scopeLogs' in line {idx}"));

            for scope in scope_logs {
                let log_records = scope
                    .get("logRecords")
                    .and_then(|v| v.as_array())
                    .unwrap_or_else(|| panic!("Missing or invalid 'logRecords' in line {idx}"));

                for record in log_records {
                    let timestamp = extract_timestamp(record);
                    let sanitized = sanitize(record.clone());
                    if result.insert(timestamp, sanitized).is_some() {
                        panic!("Duplicate timestamp value {timestamp}");
                    }
                }
            }
        }
    }
    result
}

#[test]
fn vector_sink_otel_sink_logs_match() {
    let collector_logs =
        parse_log_records(read_file_helper("collector-file-exporter.log").unwrap());
    let vector_logs = parse_log_records(read_file_helper("vector-file-sink.log").unwrap());

    assert_eq!(
        collector_logs.len(),
        EXPECTED_LOG_COUNT,
        "Collector produced {} log records, expected {EXPECTED_LOG_COUNT}",
        collector_logs.len()
    );
    assert_eq!(
        vector_logs.len(),
        EXPECTED_LOG_COUNT,
        "Vector produced {} log records, expected {EXPECTED_LOG_COUNT}",
        vector_logs.len()
    );

    // Compare logs by matching timestamps
    for (timestamp, collector_log) in &collector_logs {
        let vector_log = vector_logs
            .get(timestamp)
            .unwrap_or_else(|| panic!("Missing timestamp {timestamp} in vector logs"));

        let collector_normalized = normalize_numbers_to_strings(collector_log);
        let vector_normalized = normalize_numbers_to_strings(vector_log);

        assert_eq!(
            collector_normalized, vector_normalized,
            "Log mismatch for timestamp {timestamp}"
        );
    }
}
