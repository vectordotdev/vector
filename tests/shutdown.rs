#![cfg(all(feature = "sources", feature = "sinks-console"))]

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
use vector::test_util::temp_file;

const STDIO_CONFIG: &'static str = r#"
[sources.in]
    type = "stdin"

[sinks.out]
    inputs = ["in"]
    type = "console"
    encoding = "text"
"#;

const ALL_SOURCE_CONFIG: &'static str = r#"
    [sources.in0]
        type = "stdin"

#    [sources.in1]
#        type = "docker"

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

#    [sources.in5]
#        type = "journald"
#        include_units = [".dummy.vector.service"]

#    [sources.in6]
#        type = "kafka"
#        bootstrap_servers = "localhost:7006"
#        group_id = "consumer-group-name"
#        topics = ["topic-1"]

    [sources.in7]
        type = "logplex"
        address = "0.0.0.0:7007"

#    [sources.in8]
#        type = "prometheus"
#        hosts = ["http://localhost:7008"]
#
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


[sinks.out]
#    inputs = ["in0","in1","in2","in3","in4","in5","in6","in7","in8","in9","in10","in11","in12","in13","in14","in15","in16","in17"]
    inputs = ["in0","in2","in3","in4","in7","in9","in10","in11","in12","in13","in14","in15","in16","in17"]
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

#[test]
fn auto_shutdown() {
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("-c").arg(create_file(STDIO_CONFIG)).arg("--quiet");

    // Once `stdin source` reads whole buffer it will automatically
    // shutdown which will also cause vector process to shutdown
    // because all sources have shutdown.
    let assert = cmd.with_stdin().buffer("42").assert();

    assert.success().stdout("42\n");
}

#[test]
fn timely_shutdown() {
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("-c")
        .arg(create_file(ALL_SOURCE_CONFIG))
        .env("SYSLOG_UNIX_PATH", temp_file())
        .env("SOCKET_UNIX_PATH", temp_file());

    let mut vector = cmd.spawn().unwrap();

    // Give time vector to start.
    sleep(Duration::from_secs(2));

    // Signal shutdown
    kill(Pid::from_raw(vector.id() as i32), Signal::SIGTERM).unwrap();

    // Time shutdown.
    let now = Instant::now();

    // Wait for shutdown
    assert!(vector.wait().unwrap().success());

    assert!(now.elapsed() < Duration::from_secs(3));
}
