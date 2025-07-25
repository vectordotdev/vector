use serde_json::Value;
use std::collections::BTreeMap;
use std::process::Command;
use std::time::Duration;

const SLEEP_BETWEEN_ATTEMPTS: Duration = Duration::from_secs(1);
const EXPECTED_LOG_COUNT: usize = 100;

const OTEL_COLLECTOR_SINK_CONTAINER: &str = "otel-collector-sink";
const OTEL_COLLECTOR_SINK_LOG_PATH: &str = "/tmp/file-exporter.log";
const VECTOR_CONTAINER: &str = "vector";
const VECTOR_LOG_PATH: &str = "/tmp/file-sink.log";

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

fn read_log_records(container_name: &str, container_path: &str) -> BTreeMap<u64, Value> {
    let contents = copy_file_from_container(container_name, container_path);

    let mut result = BTreeMap::new();

    for (idx, line) in contents.lines().enumerate() {
        let root: Value = serde_json::from_str(line).unwrap_or_else(|_| {
            panic!(
                "Malformed JSON on line {} in {container_path}\nLine: {line}",
                idx + 1,
            )
        });

        let resource_logs = root
            .get("resourceLogs")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| {
                panic!(
                    "Missing or invalid 'resourceLogs' in line {container_path} of {}",
                    idx + 1,
                )
            });

        for resource in resource_logs {
            let scope_logs = resource
                .get("scopeLogs")
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| {
                    panic!(
                        "Missing or invalid 'scopeLogs' in line {} of {container_path}",
                        idx + 1,
                    )
                });

            for scope in scope_logs {
                let log_records = scope
                    .get("logRecords")
                    .and_then(|v| v.as_array())
                    .unwrap_or_else(|| {
                        panic!(
                            "Missing or invalid 'logRecords' in line {} of {container_path}",
                            idx + 1,
                        )
                    });

                for record in log_records {
                    let count = extract_count(record);
                    let sanitized = sanitize(record.clone());
                    if result.insert(count, sanitized).is_some() {
                        panic!("Duplicate count value {count} in {container_path}");
                    }
                }
            }
        }
    }
    result
}

fn copy_file_from_container(container: &str, container_path: &str) -> String {
    let tmpfile = tempfile::NamedTempFile::new().expect("Failed to create temporary file");
    let local_path = tmpfile.path().to_path_buf();

    let mut attempts = 0;
    loop {
        let status = Command::new("docker")
            .args([
                "cp",
                &format!("{container}:{container_path}"),
                local_path.to_str().expect("Invalid temp path"),
            ])
            .status()
            .expect("Failed to run docker cp command");

        if status.success() {
            break;
        }

        attempts += 1;
        if attempts >= 5 {
            panic!("docker cp failed with status: {:?}", status);
        }

        std::thread::sleep(SLEEP_BETWEEN_ATTEMPTS);
    }

    std::fs::read_to_string(&local_path).expect("Failed to read copied log file")
}

/// # Panics
/// After the timeout, this function will panic if both logs are not ready.
fn wait_for_logs() -> (BTreeMap<u64, Value>, BTreeMap<u64, Value>) {
    let collector_logs =
        read_log_records(OTEL_COLLECTOR_SINK_CONTAINER, OTEL_COLLECTOR_SINK_LOG_PATH);
    let vector_logs = read_log_records(VECTOR_CONTAINER, VECTOR_LOG_PATH);

    assert_eq!(collector_logs.len(), vector_logs.len());
    (collector_logs, vector_logs)
}
