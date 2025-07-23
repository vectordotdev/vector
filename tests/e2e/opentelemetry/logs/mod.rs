use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::timeout;
const MAXIMUM_WAITING_DURATION: Duration = Duration::from_secs(30); // Adjustable timeout
const POLLING_INTERVAL: Duration = Duration::from_millis(500); // Poll every 500ms

fn output_log_path() -> PathBuf {
    let project_root = std::env::current_dir().expect("Failed to get current dir");
    project_root
        .join("tests")
        .join("data")
        .join("e2e")
        .join("opentelemetry")
        .join("logs")
        .join("output")
        .join("collector-sink.log")
}

#[tokio::test]
async fn otlp_log_reaches_collector_file_and_ids_are_monotonic() {
    // Defined in scripts/e2e/opentelemetry-logs/compose.yaml.
    let expected_number_of_logs: u64 = 100;

    let mut previous_log_id: Option<u64> = None;
    let mut total_log_lines_found = 0;

    let result = timeout(MAXIMUM_WAITING_DURATION, async {
        loop {
            let log_file_contents = match tokio::fs::read_to_string(&output_log_path()).await {
                Ok(contents) => contents,
                Err(_) => {
                    tokio::time::sleep(POLLING_INTERVAL).await;
                    continue;
                }
            };

            total_log_lines_found = 0;
            for (line_index, line_content) in log_file_contents.lines().enumerate() {
                if line_content.trim().is_empty() {
                    continue;
                }
                let json_value: Value = serde_json::from_str(line_content).unwrap_or_else(|_| panic!("Line {line_index} is not valid JSON: {line_content}"));
                // Traverse to the log.id field
                let log_id_string = json_value
                    .pointer("/resourceLogs/0/scopeLogs/0/logRecords/0/attributes")
                    .and_then(|attributes| attributes.as_array())
                    .and_then(|attributes| {
                        attributes.iter().find_map(|attribute| {
                            if attribute.get("key")?.as_str()? == "log.id" {
                                attribute.get("value")?.get("stringValue")?.as_str()
                            } else {
                                None
                            }
                        })
                    })
                    .expect("Missing log.id attribute");
                let log_id_numeric: u64 = log_id_string.parse().expect("log.id is not numeric");

                if let Some(previous) = previous_log_id {
                    assert!(
                        log_id_numeric > previous,
                        "log.id not monotonically increasing: previous={previous} current={log_id_numeric}"
                    );
                }
                previous_log_id = Some(log_id_numeric);
                total_log_lines_found += 1;
            }

            if total_log_lines_found >= expected_number_of_logs {
                break;
            }
            tokio::time::sleep(POLLING_INTERVAL).await;
        }
        total_log_lines_found
    })
    .await;

    match result {
        Ok(log_count) => assert_eq!(
            log_count, expected_number_of_logs,
            "Expected {expected_number_of_logs} logs, but found {log_count}"
        ),
        Err(_) => panic!("Test timed out after {MAXIMUM_WAITING_DURATION:?}"),
    }

    assert!(
        total_log_lines_found > 0,
        "No log lines found in collector sink output"
    );
}
