use assert_cmd::prelude::*;
use std::process::Command;

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
        assert!(!present, "Log detected in output line: {:?}", line);
    }
}

#[test]
fn clean_list() {
    assert_no_log_lines(run_command(vec!["list"]));
}

#[test]
fn clean_generate() {
    assert_no_log_lines(run_command(vec!["generate", "stdin//console"]));
}
