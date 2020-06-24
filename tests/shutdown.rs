#![cfg(all(feature = "shutdown-tests"))]

use assert_cmd::prelude::*;
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid,
};
use std::{
    fs::OpenOptions,
    io::Write,
    net::SocketAddr,
    path::PathBuf,
    process::Command,
    thread::sleep,
    time::{Duration, Instant},
};
use vector::test_util::{next_addr, temp_dir, temp_file};

const STDIO_CONFIG: &'static str = r#"
    data_dir = "${VECTOR_DATA_DIR}"

    [sources.in]
        type = "stdin"

    [sinks.out]
        inputs = ["in"]
        type = "console"
        encoding = "text"
"#;

const PROMETHEUS_SINK_CONFIG: &'static str = r#"
    data_dir = "${VECTOR_DATA_DIR}"

    [sources.in]
        type = "stdin"

    [transforms.log_to_metric]
        type = "log_to_metric"
        inputs = ["in"]

        [[transforms.log_to_metric.metrics]]
          type = "histogram"
          field = "time"

    [sinks.out]
        type = "prometheus"
        inputs = ["log_to_metric"]
        address = "${VECTOR_TEST_ADDRESS}"
        namespace = "service"
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

fn source_vector(source: &str) -> Command {
    vector(source_config(source).as_str())
}

fn vector(config: &str) -> Command {
    vector_with_address(config, next_addr())
}

fn vector_with_address(config: &str, address: SocketAddr) -> Command {
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("-c")
        .arg(create_file(config))
        .arg("--quiet")
        .env("VECTOR_DATA_DIR", create_directory())
        .env("VECTOR_TEST_UNIX_PATH", temp_file())
        .env("VECTOR_TEST_ADDRESS", format!("{}", address));

    cmd
}

fn test_timely_shutdown(cmd: Command) {
    test_timely_shutdown_with_sub(cmd, || ());
}

fn test_timely_shutdown_with_sub(mut cmd: Command, sub: impl FnOnce()) {
    let mut vector = cmd.stdin(std::process::Stdio::piped()).spawn().unwrap();

    // Give vector time to start.
    sleep(Duration::from_secs(1));

    // Check if vector is still running
    assert_eq!(None, vector.try_wait().unwrap(), "Vector exited too early.");

    // Run sub while this vector is running.
    sub();

    // Signal shutdown
    kill(Pid::from_raw(vector.id() as i32), Signal::SIGTERM).unwrap();

    // Time shutdown.
    let now = Instant::now();

    // Wait for shutdown
    assert!(
        vector.wait().unwrap().success(),
        "Vector didn't exit successfully."
    );

    // Check if vector has shutdown in a reasonable time
    assert!(
        now.elapsed() < Duration::from_secs(3),
        "Shutdown lasted for more than 3 seconds."
    );
}

#[test]
fn auto_shutdown() {
    let mut cmd = assert_cmd::Command::cargo_bin("vector").unwrap();
    cmd.arg("--quiet")
        .arg("-c")
        .arg(create_file(STDIO_CONFIG))
        .env("VECTOR_DATA_DIR", create_directory());

    // Once `stdin source` reads whole buffer it will automatically
    // shutdown which will also cause vector process to shutdown
    // because all sources have shutdown.
    let assert = cmd.write_stdin("42").assert();

    assert.success().stdout("42\n");
}

#[test]
fn timely_shutdown_stdin() {
    test_timely_shutdown(source_vector(r#"type = "stdin""#));
}

#[test]
fn timely_shutdown_file() {
    test_timely_shutdown(source_vector(
        r#"
    type = "file"
    include = ["./*.log_dummy"]"#,
    ));
}

#[test]
fn timely_shutdown_generator() {
    test_timely_shutdown(source_vector(
        r#"
    type = "generator"
    batch_interval = 1.0 # optional, no default
    lines = []"#,
    ));
}

#[test]
fn timely_shutdown_http() {
    test_timely_shutdown(source_vector(
        r#"
    type = "http"
    address = "${VECTOR_TEST_ADDRESS}""#,
    ));
}

#[test]
fn timely_shutdown_logplex() {
    test_timely_shutdown(source_vector(
        r#"
    type = "logplex"
    address = "${VECTOR_TEST_ADDRESS}""#,
    ));
}

#[test]
fn timely_shutdown_docker() {
    test_timely_shutdown(source_vector(r#"type = "docker""#));
}

#[test]
fn timely_shutdown_journald() {
    test_timely_shutdown(source_vector(
        r#"
    type = "journald"
    include_units = []"#,
    ));
}

#[test]
fn timely_shutdown_prometheus() {
    let address = next_addr();
    test_timely_shutdown_with_sub(vector_with_address(PROMETHEUS_SINK_CONFIG, address), || {
        test_timely_shutdown(vector_with_address(
            source_config(
                r#"
        type = "prometheus"
        hosts = ["http://${VECTOR_TEST_ADDRESS}"]"#,
            )
            .as_str(),
            address,
        ));
    });
}

#[test]
fn timely_shutdown_kafka() {
    test_timely_shutdown(source_vector(
        r#"
        type = "kafka"
        bootstrap_servers = "localhost:9092"
        group_id = "consumer-group-name"
        topics = ["topic-1"]"#,
    ));
}

#[test]
fn timely_shutdown_socket_tcp() {
    test_timely_shutdown(source_vector(
        r#"
        type = "socket"
        address = "${VECTOR_TEST_ADDRESS}"
        mode = "tcp""#,
    ));
}

#[test]
fn timely_shutdown_socket_udp() {
    test_timely_shutdown(source_vector(
        r#"
        type = "socket"
        address = "${VECTOR_TEST_ADDRESS}"
        mode = "udp""#,
    ));
}

#[test]
fn timely_shutdown_socket_unix() {
    test_timely_shutdown(source_vector(
        r#"
        type = "socket"
        path = "${VECTOR_TEST_UNIX_PATH}"
        mode = "unix""#,
    ));
}

#[test]
fn timely_shutdown_splunk_hec() {
    test_timely_shutdown(source_vector(
        r#"
    type = "splunk_hec"
    address = "${VECTOR_TEST_ADDRESS}""#,
    ));
}

#[test]
fn timely_shutdown_statsd() {
    test_timely_shutdown(source_vector(
        r#"
    type = "statsd"
    address = "${VECTOR_TEST_ADDRESS}""#,
    ));
}

#[test]
fn timely_shutdown_syslog_tcp() {
    test_timely_shutdown(source_vector(
        r#"
        type = "syslog"
        address = "${VECTOR_TEST_ADDRESS}"
        mode = "tcp""#,
    ));
}

#[test]
fn timely_shutdown_syslog_udp() {
    test_timely_shutdown(source_vector(
        r#"
        type = "syslog"
        address = "${VECTOR_TEST_ADDRESS}"
        mode = "udp""#,
    ));
}

#[test]
fn timely_shutdown_syslog_unix() {
    test_timely_shutdown(source_vector(
        r#"
        type = "syslog"
        path = "${VECTOR_TEST_UNIX_PATH}"
        mode = "unix""#,
    ));
}

#[test]
fn timely_shutdown_vector() {
    test_timely_shutdown(source_vector(
        r#"
    type = "vector"
    address = "${VECTOR_TEST_ADDRESS}""#,
    ));
}

#[test]
fn timely_shutdown_internal_metrics() {
    test_timely_shutdown(source_vector(
        r#"
    type = "internal_metrics""#,
    ));
}

#[test]
fn timely_shutdown_lua_timer() {
    test_timely_shutdown(vector(
        r#"
[sources.source]
   type = "stdin"

[transforms.transform]
  type = "lua"
  inputs = ["source"]
  version = "2"

  hooks.process = "process"

  timers = [
  {interval_seconds = 5, handler = "timer_handler"}
  ]

  source = """
    function process(event, emit)
      emit(event)
    end

    function timer_handler(emit)
      event = {
        log = {
          message = "Heartbeat",
        }
      }
      return event
    end
  """

[sinks.sink]
  type = "console"
  inputs = ["transform"]
  encoding = "text"
  target = "stdout"
"#,
    ));
}
