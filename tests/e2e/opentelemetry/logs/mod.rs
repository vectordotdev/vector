use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

const EXPECTED_LOG_COUNT: usize = 100;

fn log_output_dir() -> PathBuf {
    std::env::current_dir()
        .expect("Failed to get current dir")
        .join("tests")
        .join("data")
        .join("e2e")
        .join("opentelemetry")
        .join("logs")
        .join("output")
}

fn collector_log_path() -> PathBuf {
    log_output_dir().join("collector-file-exporter.log")
}

fn vector_log_path() -> PathBuf {
    log_output_dir().join("vector-file-sink.log")
}

use std::{fs, io, path::Path, thread, time::Duration};

pub fn read_file_contents(path: &Path) -> Result<String, io::Error> {
    let max_retries = 5;
    let retry_delay = Duration::from_secs(2);
    let mut last_err: Option<io::Error> = None;
    for attempt in 1..=max_retries {
        match fs::read_to_string(path) {
            Ok(contents) => return Ok(contents),
            Err(e) => {
                eprintln!(
                    "Attempt {attempt}/{max_retries}: Failed to read file '{}': {e}",
                    path.display()
                );
                last_err = Some(e);
                if attempt < max_retries {
                    thread::sleep(retry_delay);
                    continue;
                }
            }
        }
    }
    Err(last_err.unwrap())
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

fn read_log_records(path: &Path) -> BTreeMap<u64, Value> {
    let content = read_file_contents(path).unwrap();

    let mut result = BTreeMap::new();

    for (idx, line) in content.lines().enumerate() {
        let root: Value = serde_json::from_str(line).unwrap_or_else(|_| {
            panic!(
                "Malformed JSON on line {} in {}\nLine: {line}",
                idx + 1,
                path.display()
            )
        });

        let resource_logs = root
            .get("resourceLogs")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| {
                panic!(
                    "Missing or invalid 'resourceLogs' in line {} of {}",
                    idx + 1,
                    path.display()
                )
            });

        for resource in resource_logs {
            let scope_logs = resource
                .get("scopeLogs")
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| {
                    panic!(
                        "Missing or invalid 'scopeLogs' in line {} of {}",
                        idx + 1,
                        path.display()
                    )
                });

            for scope in scope_logs {
                let log_records = scope
                    .get("logRecords")
                    .and_then(|v| v.as_array())
                    .unwrap_or_else(|| {
                        panic!(
                            "Missing or invalid 'logRecords' in line {} of {}",
                            idx + 1,
                            path.display()
                        )
                    });

                for record in log_records {
                    let count = extract_count(record);
                    let sanitized = sanitize(record.clone());
                    if result.insert(count, sanitized).is_some() {
                        panic!("Duplicate count value {count} in {}", path.display());
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
    let collector_logs = read_log_records(&collector_log_path());
    let vector_logs = read_log_records(&vector_log_path());

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
