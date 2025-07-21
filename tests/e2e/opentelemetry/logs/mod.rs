use std::{fs, thread, time::Duration};

#[test]
fn otlp_log_reaches_collector_file() {
    // 1. Wait for the pipeline to process logs (adjust as needed)
    thread::sleep(Duration::from_secs(5));

    // 2. Read the collector sink's output file
    let log_path = "./pront/otel/otel/output/logs.log";
    let contents = fs::read_to_string(log_path)
        .expect("Failed to read otel collector sink log file");

    // 3. Assert that a known log message is present
    // Replace this string with a unique message from your generator or remap
    let expected = "opentelemetry-vector-e2e";
    assert!(
        contents.contains(expected),
        "Expected log message '{}' not found in collector sink output:\n{}",
        expected,
        contents
    );
}
