use assert_cmd::prelude::*;
use std::{fs::read_dir, process::Command};

mod support;

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

fn source_config(source: &str) -> String {
    format!(
        r#"
data_dir = "${{VECTOR_DATA_DIR}}"

[sources.in]
{}

[sinks.out]
    inputs = ["in"]
    type = "blackhole"
    print_amount = 10000
"#,
        source
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
    let dir = support::create_directory();
    let mut path = dir.clone();
    path.push("tmp");
    path.set_extension("data");
    support::overwrite_file(path.clone(), "");

    // Config with some componenets that write to file system.
    let config = support::create_file(
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

    assert_no_log_lines(output.stdout);

    // Assert that data folder didn't change
    assert_eq!(
        vec![path],
        read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>()
    );
}
