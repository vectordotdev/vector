use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::info;

const MAXIMUM_WAITING_DURATION: Duration = Duration::from_secs(30);
const POLLING_INTERVAL: Duration = Duration::from_millis(500);
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
    value.as_object_mut().map(|obj| {
        obj.remove("traceId");
        obj.remove("spanId");
    });
    value
}

async fn read_log_records(path: &PathBuf) -> BTreeMap<u64, Value> {
    let content = tokio::fs::read_to_string(path)
        .await
        .unwrap_or_else(|_| panic!("Failed to read log file {}", path.display()));

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
async fn wait_for_logs() -> (BTreeMap<u64, Value>, BTreeMap<u64, Value>) {
    let mut collector_acc: BTreeMap<u64, Value> = BTreeMap::new();
    let mut vector_acc: BTreeMap<u64, Value> = BTreeMap::new();

    timeout(MAXIMUM_WAITING_DURATION, async {
        loop {
            let collector_read = read_log_records(&collector_log_path()).await;
            let vector_read = read_log_records(&vector_log_path()).await;

            // Merge into accumulators, skipping duplicates
            for (k, v) in collector_read {
                collector_acc.entry(k).or_insert(v);
            }
            for (k, v) in vector_read {
                vector_acc.entry(k).or_insert(v);
            }

            let c_len = collector_acc.len();
            let v_len = vector_acc.len();

            if c_len >= EXPECTED_LOG_COUNT && v_len >= EXPECTED_LOG_COUNT {
                return (collector_acc, vector_acc);
            }

            info!("Waiting for logs... collector: {c_len}, vector: {v_len}");

            sleep(POLLING_INTERVAL).await;
        }
    })
    .await
    .expect("Timed out waiting for both log files to contain sufficient records")
}

#[tokio::test]
async fn vector_sink_otel_sink_logs_match() {
    let (collector_log_records, vector_log_records) = wait_for_logs().await;

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

    println!("Collector logs: {collector_log_records:#?}");
    println!("V logs: {vector_log_records:#?}");
    for count in 0..=EXPECTED_LOG_COUNT as u64 {
        let c = collector_log_records.get(&count).unwrap();
        let v = vector_log_records.get(&count).unwrap();
        assert_eq!(c, v);
    }
}
