#![allow(clippy::print_stdout)]
use std::{collections::HashSet, fs::read_dir, process::Command};

use assert_cmd::prelude::*;
use indoc::indoc;

use crate::{create_directory, create_file, overwrite_file};

const FAILING_HEALTHCHECK: &str = r#"
data_dir = "${VECTOR_DATA_DIR}"

[sources.in]
    type = "demo_logs"
    lines = ["log"]
    format = "shuffle"

[sinks.out]
    inputs = ["in"]
    type = "socket"
    address = "192.168.0.0:62178"
    encoding.codec = "json" # required
    mode = "tcp"
"#;

/// Returns `stdout` of `vector arguments`
fn run_command(arguments: Vec<&str>) -> Vec<u8> {
    let mut cmd = Command::cargo_bin("vector").unwrap();
    for arg in arguments {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute process");

    output.stdout
}

fn assert_no_log_lines(output: Vec<u8>) {
    let output = String::from_utf8(output).expect("Vector output isn't a valid utf8 string");

    // Assert there are no lines with keywords
    let keywords = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];
    for line in output.lines() {
        let present = keywords.iter().any(|word| line.contains(word));
        assert!(!present, "Log detected in output line: {line:?}");
    }
}

fn source_config(source: &str) -> String {
    format!(
        r#"
data_dir = "${{VECTOR_DATA_DIR}}"

[sources.in]
{source}

[sinks.out]
    inputs = ["in"]
    type = "blackhole"
"#
    )
}

#[test]
fn clean_list() {
    assert_no_log_lines(run_command(vec!["list"]));
}

#[test]
fn clean_generate() {
    assert_no_log_lines(run_command(vec!["generate", "stdin//console"]));
}

#[test]
fn validate_cleanup() {
    // Create component directories with some file.
    let dir = create_directory();
    let mut path = dir.clone();
    path.push("tmp");
    path.set_extension("data");
    overwrite_file(path.clone(), "");

    // Config with some components that write to file system.
    let config = create_file(
        source_config(
            r#"
    type = "file"
    include = ["./*.log_dummy"]"#,
        )
        .as_str(),
    );

    // Run vector
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("validate")
        .arg(config)
        .env("VECTOR_DATA_DIR", dir.clone());

    let output = cmd.output().expect("Failed to execute process");
    println!(
        "{}",
        String::from_utf8(output.stdout.clone()).expect("Vector output isn't a valid utf8 string")
    );

    assert_no_log_lines(output.stdout);
    assert_eq!(output.status.code(), Some(0));

    // Assert that data folder didn't change
    assert_eq!(
        HashSet::from([path]),
        read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<HashSet<_>>()
    );
}

#[test]
fn validate_failing_healthcheck() {
    assert_eq!(validate(FAILING_HEALTHCHECK), exitcode::CONFIG);
}

#[test]
fn validate_ignore_healthcheck() {
    assert_eq!(
        validate(&format!(
            r#"
        healthchecks.enabled = false
        {FAILING_HEALTHCHECK}
        "#
        )),
        exitcode::OK
    );
}

#[test]
fn test_command_no_escape_codes_in_output() {
    // A config with an unhandled fallible VRL function call (missing `!`).
    // This triggers a VRL compilation error reported through the test runner.
    let config = create_file(indoc! {"
        transforms:
          broken:
            inputs: []
            type: remap
            source: .foo = to_int(.bar)
        tests:
          - name: broken_test
            input:
              insert_at: broken
              type: log
              log_fields:
                bar: not_an_int
            outputs:
              - extract_from: broken
                conditions:
                  - type: vrl
                    source: 'true'
    "});

    let mut cmd = Command::cargo_bin("vector").unwrap();
    // Force colors on so VRL diagnostics contain ANSI codes. Without this,
    // the subprocess detects a non-TTY and disables colors, which would make
    // the test pass even if error! was used instead of eprintln!.
    cmd.arg("--color").arg("always").arg("test").arg(config);

    let output = cmd.output().expect("Failed to execute process");
    let stdout = String::from_utf8(output.stdout).expect("stdout isn't valid utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr isn't valid utf8");

    // The command should fail
    assert_ne!(output.status.code(), Some(0));

    // Neither stdout nor stderr should contain literal escape code text.
    // The error! macro escapes ANSI escape bytes into literal "\x1b" text,
    // while eprintln! passes them through as raw bytes.
    assert!(
        !stdout.contains(r"\x1b"),
        "stdout contains literal \\x1b escape codes: {stdout}"
    );
    assert!(
        !stderr.contains(r"\x1b"),
        "stderr contains literal \\x1b escape codes: {stderr}"
    );
}

fn validate(config: &str) -> i32 {
    let dir = create_directory();

    // Config with some components that write to file system.
    let config = create_file(config);

    // Run vector
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("validate").arg(config).env("VECTOR_DATA_DIR", dir);

    let output = cmd.output().unwrap();
    println!(
        "{}",
        String::from_utf8(output.stdout).expect("Vector output isn't a valid utf8 string")
    );
    output.status.code().unwrap()
}
