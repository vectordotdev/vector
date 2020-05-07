#![cfg(all(
    feature = "sources",
    feature = "sinks-console",
    feature = "sinks-prometheus"
))]

extern crate assert_cmd;

use assert_cmd::prelude::*;
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid,
};
use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    process::Command,
    thread::sleep,
    time::{Duration, Instant},
};
use vector::test_util::{temp_dir, temp_file};

const STDIO_CONFIG: &'static str = r#"
    data_dir = "${VECTOR_DATA_DIR}"

    [sources.in]
        type = "stdin"

    [sinks.out]
        inputs = ["in"]
        type = "console"
        encoding = "text"
"#;

const ALL_SOURCE_CONFIG: &'static str = r#"
    data_dir = "${VECTOR_DATA_DIR}"

    [sources.in0]
        type = "stdin"

    [sources.in2]
        type = "file" # required
        include = ["./*.log_dummy"]
        
    [sources.in3]
        type = "generator"
        batch_interval = 1.0 # optional, no default
        lines = []

    [sources.in4]
        type = "http"
        address = "0.0.0.0:7004"

    [sources.in7]
        type = "logplex"
        address = "0.0.0.0:7007"

    [sources.in8]
        type = "prometheus"
        hosts = ["http://localhost:7008"]

    [sources.in9]
        type = "socket"
        address = "0.0.0.0:7009"
        mode = "tcp"

    [sources.in10]
        type = "socket"
        address = "0.0.0.0:7010"
        mode = "udp"

    [sources.in11]
        type = "socket"
        path = "${SOCKET_UNIX_PATH}"
        mode = "unix"

    [sources.in12]
        type = "splunk_hec"
        address = "0.0.0.0:7012"

    [sources.in13]
        type = "statsd"
        address = "127.0.0.1:7013"

    [sources.in14]
        type = "syslog"
        address = "0.0.0.0:7014"
        mode = "tcp"

    [sources.in15]
        type = "syslog"
        address = "0.0.0.0:7015"
        mode = "udp"

    [sources.in16]
        type = "syslog"
        mode = "unix"
        path = "${SYSLOG_UNIX_PATH}"

    [sources.in17]
        type = "vector"
        address = "0.0.0.0:7017"


    [sinks.out0]
        inputs = ["in0","in2","in3","in4","in7","in9","in10","in11","in12","in13","in14","in15","in16","in17"]
        type = "console"
        encoding = "text"

    [sinks.out1]
        type = "prometheus" 
        inputs = ["in8"]
        address = "0.0.0.0:7008" 
        namespace = "service" 
"#;

#[cfg(feature = "shutdown-tests")]
const SOURCE_DOCKER_CONFIG: &'static str = r#"
    data_dir = "${VECTOR_DATA_DIR}"

    [sources.in1]
        type = "docker"

    [sources.in6]
        type = "kafka"
        bootstrap_servers = "localhost:9092"
        group_id = "consumer-group-name"
        topics = ["topic-1"]
  
    [sinks.out]
        inputs = ["in1","in6"]
        type = "console"
        encoding = "text"
"#;

/// Creates a file with given content
fn create_file(config: &str) -> PathBuf {
    let path = temp_file();
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path.clone())
        .unwrap();

    file.write_all(config.as_bytes()).unwrap();
    file.flush().unwrap();

    path
}

fn create_directory() -> PathBuf {
    let path = temp_dir();
    Command::new("mkdir").arg(path.clone()).assert().success();
    path
}

fn test_timely_shutdown(config: &str) {
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("-c")
        .arg(create_file(config))
        .env("VECTOR_DATA_DIR", create_directory())
        .env("SYSLOG_UNIX_PATH", temp_file())
        .env("SOCKET_UNIX_PATH", temp_file());

    let mut vector = cmd.spawn().unwrap();

    // Give vector time to start.
    sleep(Duration::from_secs(2));

    // Signal shutdown
    kill(Pid::from_raw(vector.id() as i32), Signal::SIGTERM).unwrap();

    // Time shutdown.
    let now = Instant::now();

    // Wait for shutdown
    assert!(vector.wait().unwrap().success());

    assert!(now.elapsed() < Duration::from_secs(3));
}

#[test]
fn auto_shutdown() {
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("-c")
        .arg("--quiet")
        .arg(create_file(STDIO_CONFIG))
        .env("VECTOR_DATA_DIR", create_directory());

    // Once `stdin source` reads whole buffer it will automatically
    // shutdown which will also cause vector process to shutdown
    // because all sources have shutdown.
    let assert = cmd.with_stdin().buffer("42").assert();

    assert.success().stdout("42\n");
}

#[test]
fn timely_shutdown() {
    test_timely_shutdown(ALL_SOURCE_CONFIG);
}

#[cfg(feature = "shutdown-tests")]
#[test]
fn timely_docker_shutdown() {
    test_timely_shutdown(SOURCE_DOCKER_CONFIG);
}
