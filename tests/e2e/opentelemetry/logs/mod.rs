use serde_json::Value;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::process::Command;

const EXPECTED_LOG_COUNT: usize = 100;

fn read_file_helper(filename: &str) -> Result<String, io::Error> {
    let local_path = Path::new("/output/opentelemetry-logs").join(filename);
    if local_path.exists() {
        // Running inside the runner container, volume is mounted
        std::fs::read_to_string(local_path)
    } else {
        // Running on hostno-eno
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
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "docker run failed: {}\n{}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr)
                ),
            ));
        }

        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

fn extract_count(value: &Value) -> u64 {
    value
        .get("attributes")
        .and_then(|attrs| attrs.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|attr| {
                if attr.get("key")?.as_str()? == "count" {
                    attr.get("value")?
                        .get("intValue")?
                        .as_str()
                        .and_then(|s| s.parse::<u64>().ok())
                        .or_else(|| attr.get("value")?.get("intValue")?.as_u64())
                } else {
                    None
                }
            })
        })
        .expect("Missing or invalid 'count' attribute in log record")
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
                    let count = extract_count(record);
                    let sanitized = sanitize(record.clone());
                    if result.insert(count, sanitized).is_some() {
                        panic!("Duplicate count value {count}");
                    }
                }
            }
        }
    }
    result
}

/// # Panics
/// After the timeout, this function will panic if both logs are not ready.
fn wait_for_logs() -> (BTreeMap<u64, Value>, BTreeMap<u64, Value>) {
    let collector_logs =
        parse_log_records(read_file_helper("collector-file-exporter.log").unwrap());
    let vector_logs = parse_log_records(read_file_helper("vector-file-sink.log").unwrap());

    assert_eq!(
        collector_logs.len(),
        EXPECTED_LOG_COUNT,
        "Collector did not produce expected number of log records"
    );
    assert_eq!(
        vector_logs.len(),
        EXPECTED_LOG_COUNT,
        "Vector did not produce expected number of log records"
    );

    (collector_logs, vector_logs)
}

#[test]
fn vector_sink_otel_sink_logs_match() {
    let (collector_log_records, vector_log_records) = wait_for_logs();

    assert_eq!(
        collector_log_records.len(),
        EXPECTED_LOG_COUNT,
        "Collector did not produce expected number of log records"
    );
    assert_eq!(
        vector_log_records.len(),
        EXPECTED_LOG_COUNT,
        "Vector did not produce expected number of log records"
    );

    for count in 0..EXPECTED_LOG_COUNT as u64 {
        let collector_log = collector_log_records
            .get(&count)
            .unwrap_or_else(|| panic!("Missing {count}) key"));
        let vector_log = vector_log_records
            .get(&count)
            .unwrap_or_else(|| panic!("Missing {count}) key"));
        let collector_log_normalized = normalize_numbers_to_strings(collector_log);
        let vector_log_normalized = normalize_numbers_to_strings(vector_log);
        assert_eq!(collector_log_normalized, vector_log_normalized);
    }
}
