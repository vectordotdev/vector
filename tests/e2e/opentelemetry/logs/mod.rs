use serde_json::Value;
use std::{fs, thread, time::Duration};

#[test]
fn otlp_log_reaches_collector_file_and_ids_are_monotonic() {
    // Wait for the pipeline to process logs
    thread::sleep(Duration::from_secs(5));

    let log_path = "./pront/otel/otel/output/logs.log";
    let contents = fs::read_to_string(log_path)
        .expect("Failed to read otel collector sink log file");

    let mut last_id: Option<u64> = None;
    let mut line_count = 0;

    for (i, line) in contents.lines().enumerate() {
        if line.trim().is_empty() { continue; }
        let v: Value = serde_json::from_str(line)
            .expect(&format!("Line {} is not valid JSON: {}", i, line));
        // Traverse to the log.id field
        let log_id = v.pointer("/resourceLogs/0/scopeLogs/0/logRecords/0/attributes")
            .and_then(|attrs| attrs.as_array())
            .and_then(|attrs| {
                attrs.iter().find_map(|attr| {
                    if attr.get("key")?.as_str()? == "log.id" {
                        attr.get("value")?.get("stringValue")?.as_str()
                    } else {
                        None
                    }
                })
            })
            .expect("Missing log.id attribute");
        let log_id_num: u64 = log_id.parse().expect("log.id is not numeric");

        if let Some(prev) = last_id {
            assert!(
                log_id_num > prev,
                "log.id not monotonically increasing: prev={} current={}", prev, log_id_num
            );
        }
        last_id = Some(log_id_num);
        line_count += 1;
    }

    assert!(line_count > 0, "No log lines found in collector sink output");
}
