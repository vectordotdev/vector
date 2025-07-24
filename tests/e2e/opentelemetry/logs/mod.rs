use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use tokio::time::timeout;

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

async fn try_read_lines(path: &PathBuf) -> Option<Vec<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(contents) => Some(contents.lines().map(|l| l.to_string()).collect()),
        Err(_) => None,
    }
}

/// # Panics
/// After the timeout, this function will panic.
async fn wait_for_logs() -> (Vec<String>, Vec<String>) {
    timeout(MAXIMUM_WAITING_DURATION, async {
        loop {
            let collector_lines = try_read_lines(&collector_log_path()).await;
            let vector_lines = try_read_lines(&vector_log_path()).await;

            if let (Some(c_lines), Some(v_lines)) = (collector_lines, vector_lines) {
                if c_lines.len() >= EXPECTED_LOG_COUNT && v_lines.len() >= EXPECTED_LOG_COUNT {
                    return (c_lines, v_lines);
                }
            }

            sleep(POLLING_INTERVAL).await;
        }
    })
    .await
    .expect("Timed out waiting for both log files")
}

#[tokio::test]
async fn vector_sink_otel_sink_logs_match() {
    let (collector_lines, vector_lines) = wait_for_logs().await;

    assert_eq!(
        collector_lines.len(),
        EXPECTED_LOG_COUNT,
        "Collector did not produce expected number of logs"
    );
    assert_eq!(
        vector_lines.len(),
        EXPECTED_LOG_COUNT,
        "Vector did not produce expected number of logs"
    );

    let collector_set: BTreeSet<_> = collector_lines.into_iter().collect();
    let vector_set: BTreeSet<_> = vector_lines.into_iter().collect();

    assert_eq!(
        collector_set, vector_set,
        "Collector and Vector logs do not match exactly"
    );
}
