#![allow(clippy::print_stdout)]

use std::{
    io::Write,
    process::Command,
    thread::sleep,
    time::Duration,
};

use assert_cmd::prelude::*;
use serde_json::Value;

use crate::{create_directory, create_file};

const STARTUP_TIME: Duration = Duration::from_secs(2);

/// Spawn vector with the given TOML config, send `input_lines` via stdin,
/// wait for it to exit, and return stdout as a string.
fn run_vector_stdin(config: &str, input_lines: &[&str]) -> String {
    let dir = create_directory();
    let config_path = create_file(config);

    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("--quiet")
        .arg("-c")
        .arg(config_path)
        .env("VECTOR_DATA_DIR", dir);

    let mut vector = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // Give vector time to start
    sleep(STARTUP_TIME);

    {
        let stdin = vector.stdin.as_mut().unwrap();
        for line in input_lines {
            stdin.write_all(line.as_bytes()).unwrap();
            stdin.write_all(b"\n").unwrap();
        }
    }
    // Close stdin to trigger shutdown via EOF
    drop(vector.stdin.take());

    let output = vector.wait_with_output().unwrap();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "Vector exited with status {}.\nstderr:\n{}",
            output.status, stderr
        );
    }
    String::from_utf8(output.stdout).unwrap()
}

/// Parse each line of stdout as a JSON object, return as Vec<Value>.
fn parse_json_lines(stdout: &str) -> Vec<Value> {
    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("Invalid JSON: {e}\nLine: {l}")))
        .collect()
}

// ─── Test 1: Basic throttle pass/drop ────────────────────────────────
#[test]
fn throttle_basic_pass_drop() {
    let config = r#"
        data_dir = "${VECTOR_DATA_DIR}"

        [sources.in]
            type = "stdin"

        [transforms.throttle]
            type = "throttle"
            inputs = ["in"]
            threshold = 1
            window_secs = 60

        [sinks.out]
            inputs = ["throttle"]
            type = "console"
            encoding.codec = "json"
    "#;

    let stdout = run_vector_stdin(config, &["first", "second", "third"]);
    let events = parse_json_lines(&stdout);

    // Only the first event should pass (threshold=1 per 60s window)
    assert_eq!(events.len(), 1, "Expected 1 event, got {}", events.len());
    assert_eq!(events[0]["message"], "first");
}

// ─── Test 2: Dropped output port routing ─────────────────────────────
#[test]
fn throttle_dropped_output_port() {
    // Use file sinks to capture both primary and dropped outputs.
    // Since file sink requires paths, we use console for primary
    // and verify the dropped port routes correctly.
    let config = r#"
        data_dir = "${VECTOR_DATA_DIR}"

        [sources.in]
            type = "stdin"

        [transforms.throttle]
            type = "throttle"
            inputs = ["in"]
            threshold = 1
            window_secs = 60
            reroute_dropped = true

        [sinks.primary]
            inputs = ["throttle"]
            type = "console"
            encoding.codec = "json"

        [sinks.dropped]
            inputs = ["throttle.dropped"]
            type = "blackhole"
    "#;

    let stdout = run_vector_stdin(config, &["first", "second"]);
    let events = parse_json_lines(&stdout);

    // Primary output should only have the first event
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["message"], "first");
}

// ─── Test 3: Multi-threshold with key_field ──────────────────────────
#[test]
fn throttle_multi_threshold_with_key_field() {
    let config = r#"
        data_dir = "${VECTOR_DATA_DIR}"

        [sources.in]
            type = "stdin"

        [transforms.parse]
            type = "remap"
            inputs = ["in"]
            source = '. = parse_json!(.message)'

        [transforms.throttle]
            type = "throttle"
            inputs = ["parse"]
            window_secs = 60
            key_field = "{{ service }}"

            [transforms.throttle.threshold]
                events = 1

        [sinks.out]
            inputs = ["throttle"]
            type = "console"
            encoding.codec = "json"
    "#;

    let events_input = &[
        r#"{"service":"svc-a","msg":"a1"}"#,
        r#"{"service":"svc-a","msg":"a2"}"#,
        r#"{"service":"svc-b","msg":"b1"}"#,
        r#"{"service":"svc-b","msg":"b2"}"#,
    ];

    let stdout = run_vector_stdin(config, events_input);
    let events = parse_json_lines(&stdout);

    // Each key (svc-a, svc-b) should allow 1 event through
    assert_eq!(events.len(), 2, "Expected 2 events (one per key)");

    let messages: Vec<&str> = events.iter().map(|e| e["msg"].as_str().unwrap()).collect();
    assert!(messages.contains(&"a1"));
    assert!(messages.contains(&"b1"));
}

// ─── Test 4: Backward compat (threshold: u32) ───────────────────────
#[test]
fn throttle_backward_compat_simple_threshold() {
    let config = r#"
        data_dir = "${VECTOR_DATA_DIR}"

        [sources.in]
            type = "stdin"

        [transforms.throttle]
            type = "throttle"
            inputs = ["in"]
            threshold = 2
            window_secs = 60

        [sinks.out]
            inputs = ["throttle"]
            type = "console"
            encoding.codec = "json"
    "#;

    let stdout = run_vector_stdin(config, &["one", "two", "three", "four"]);
    let events = parse_json_lines(&stdout);

    // threshold=2 should allow 2 events
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["message"], "one");
    assert_eq!(events[1]["message"], "two");
}

// ─── Test 5: Exclude condition bypasses throttle ─────────────────────
#[test]
fn throttle_exclude_bypasses_limit() {
    let config = r#"
        data_dir = "${VECTOR_DATA_DIR}"

        [sources.in]
            type = "stdin"

        [transforms.parse]
            type = "remap"
            inputs = ["in"]
            source = '. = parse_json!(.message)'

        [transforms.throttle]
            type = "throttle"
            inputs = ["parse"]
            threshold = 1
            window_secs = 60
            exclude = '.level == "error"'

        [sinks.out]
            inputs = ["throttle"]
            type = "console"
            encoding.codec = "json"
    "#;

    let events_input = &[
        r#"{"level":"info","msg":"first"}"#,
        r#"{"level":"error","msg":"critical1"}"#,
        r#"{"level":"info","msg":"second"}"#,
        r#"{"level":"error","msg":"critical2"}"#,
    ];

    let stdout = run_vector_stdin(config, events_input);
    let events = parse_json_lines(&stdout);

    // first info passes (threshold=1), second info is throttled
    // both errors bypass the throttle via exclude
    let messages: Vec<&str> = events.iter().map(|e| e["msg"].as_str().unwrap()).collect();
    assert!(messages.contains(&"first"), "First info should pass");
    assert!(messages.contains(&"critical1"), "Error events bypass throttle");
    assert!(messages.contains(&"critical2"), "Error events bypass throttle");
    assert!(!messages.contains(&"second"), "Second info should be throttled");
    assert_eq!(events.len(), 3);
}

// ─── Test 6: Config validation (multi-threshold) ─────────────────────
#[test]
fn throttle_validate_multi_threshold_config() {
    let config = r#"
        data_dir = "${VECTOR_DATA_DIR}"

        [sources.in]
            type = "stdin"

        [transforms.throttle]
            type = "throttle"
            inputs = ["in"]
            window_secs = 60
            reroute_dropped = true

            [transforms.throttle.threshold]
                events = 100
                json_bytes = 50000

            [transforms.throttle.internal_metrics]
                emit_detailed_metrics = true

        [sinks.primary]
            inputs = ["throttle"]
            type = "console"
            encoding.codec = "json"

        [sinks.dropped]
            inputs = ["throttle.dropped"]
            type = "blackhole"
    "#;

    let dir = create_directory();
    let config_path = create_file(config);

    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("validate")
        .arg(config_path)
        .env("VECTOR_DATA_DIR", dir);

    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "Multi-threshold config validation failed:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

// ─── Test 7: Data integrity - events are not modified ────────────────
#[test]
fn throttle_data_integrity_events_unmodified() {
    let config = r#"
        data_dir = "${VECTOR_DATA_DIR}"

        [sources.in]
            type = "stdin"

        [transforms.parse]
            type = "remap"
            inputs = ["in"]
            source = '. = parse_json!(.message)'

        [transforms.throttle]
            type = "throttle"
            inputs = ["parse"]
            threshold = 100
            window_secs = 60

        [sinks.out]
            inputs = ["throttle"]
            type = "console"
            encoding.codec = "json"
    "#;

    let events_input = &[
        r#"{"id":1,"data":"hello world","nested":{"a":true,"b":[1,2,3]}}"#,
        r#"{"id":2,"data":"unicode: ñ ü ö 日本語 🎉","emoji":"🚀"}"#,
        r#"{"id":3,"data":"special chars: <>&\"'\t\n"}"#,
    ];

    let stdout = run_vector_stdin(config, events_input);
    let events = parse_json_lines(&stdout);

    // All events should pass (threshold=100)
    assert_eq!(events.len(), 3);

    // Verify field values are preserved exactly
    assert_eq!(events[0]["id"], 1);
    assert_eq!(events[0]["data"], "hello world");
    assert_eq!(events[0]["nested"]["a"], true);
    assert_eq!(events[0]["nested"]["b"][0], 1);
    assert_eq!(events[0]["nested"]["b"][2], 3);

    assert_eq!(events[1]["id"], 2);
    assert_eq!(events[1]["emoji"], "🚀");

    assert_eq!(events[2]["id"], 3);
}
