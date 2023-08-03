use std::{
    fs::create_dir,
    fs::read_dir,
    io::Write,
    net::SocketAddr,
    path::PathBuf,
    process::{Child, Command},
    thread::sleep,
    time::{Duration, Instant},
};

use assert_cmd::prelude::*;
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid,
};
use serde_json::{json, Value};
use similar_asserts::assert_eq;
use vector::test_util::{next_addr, temp_file};

use crate::{create_directory, create_file, overwrite_file};

const STARTUP_TIME: Duration = Duration::from_secs(2);
const SHUTDOWN_TIME: Duration = Duration::from_secs(4);
const RELOAD_TIME: Duration = Duration::from_secs(5);

const STDIO_CONFIG: &'static str = r#"
    data_dir = "${VECTOR_DATA_DIR}"

    [sources.in_console]
        type = "stdin"

    [sinks.out_console]
        inputs = ["in_console"]
        type = "console"
        encoding.codec = "text"
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
        type = "prometheus_exporter"
        default_namespace = "service"
        inputs = ["log_to_metric"]
        address = "${VECTOR_TEST_ADDRESS}"
"#;

fn source_config(source: &str) -> String {
    format!(
        r#"
data_dir = "${{VECTOR_DATA_DIR}}"

[sources.in]
{}

[sinks.out]
    inputs = ["in"]
    type = "blackhole"
"#,
        source
    )
}

fn source_vector(source: &str) -> Command {
    vector(source_config(source).as_str())
}

fn vector(config: &str) -> Command {
    vector_with(create_file(config), next_addr(), false)
}

fn vector_with(config_path: PathBuf, address: SocketAddr, quiet: bool) -> Command {
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("-c")
        .arg(config_path)
        .arg(if quiet { "--quiet" } else { "-v" })
        .env("VECTOR_DATA_DIR", create_directory())
        .env("VECTOR_TEST_UNIX_PATH", temp_file())
        .env("VECTOR_TEST_ADDRESS", address.to_string());

    cmd
}

fn test_timely_shutdown(cmd: Command) {
    test_timely_shutdown_with_sub(cmd, |_| ());
}

/// Returns stdout output
fn test_timely_shutdown_with_sub(mut cmd: Command, sub: impl FnOnce(&mut Child)) {
    let mut vector = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // Give vector time to start.
    sleep(STARTUP_TIME);

    // Check if vector is still running
    assert_eq!(None, vector.try_wait().unwrap(), "Vector exited too early.");

    // Run sub while this vector is running.
    sub(&mut vector);

    // Signal shutdown
    kill(Pid::from_raw(vector.id() as i32), Signal::SIGTERM).unwrap();

    // Time shutdown.
    let now = Instant::now();

    // Wait for shutdown
    let output = vector.wait_with_output().unwrap();

    // Check output
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        println!("{}", stdout);
        panic!("Vector didn't exit successfully. Status: {}", output.status);
    }

    // Check if vector has shutdown in a reasonable time
    assert!(
        now.elapsed() < SHUTDOWN_TIME,
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
fn log_schema() {
    // Vector command
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("--quiet")
        .arg("-c")
        .arg(create_file(
            r#"
        data_dir = "${VECTOR_DATA_DIR}"
        log_schema.message_key = "test_msg"

        [sources.in_console]
            type = "stdin"

        [sinks.out_console]
            inputs = ["in_console"]
            type = "console"
            encoding.codec = "json"
    "#,
        ))
        .env("VECTOR_DATA_DIR", create_directory());

    // Run vector
    let mut vector = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // Give vector time to start.
    sleep(STARTUP_TIME);

    vector
        .stdin
        .as_mut()
        .unwrap()
        .write_all("42".as_bytes())
        .unwrap();

    // Wait for shutdown
    let output = vector.wait_with_output().unwrap();
    assert!(output.status.success(), "Vector didn't exit successfully.");

    // Output
    let event: Value = serde_json::from_slice(output.stdout.as_slice()).unwrap();
    assert_eq!(event["test_msg"], json!("42"));
}

#[test]
fn log_schema_multiple_config_files() {
    // Vector command
    let mut cmd = Command::cargo_bin("vector").unwrap();

    let config_dir = create_directory();

    let sinks_config_dir = config_dir.join("sinks");
    create_dir(sinks_config_dir.clone()).unwrap();

    let sources_config_dir = config_dir.join("sources");
    create_dir(sources_config_dir.clone()).unwrap();

    let input_dir = create_directory();
    let input_file = input_dir.join("input_file");

    overwrite_file(
        config_dir.join("vector.toml"),
        r#"
    data_dir = "${VECTOR_DATA_DIR}"
    log_schema.host_key = "test_host"
    "#,
    );

    overwrite_file(
        sources_config_dir.join("in_file.toml"),
        r#"
    type = "file"
    include = ["${VECTOR_TEST_INPUT_FILE}"]
    "#,
    );

    overwrite_file(
        sinks_config_dir.join("out_console.toml"),
        r#"
    inputs = ["in_file"]
    type = "console"
    encoding.codec = "json"
    "#,
    );

    overwrite_file(
        input_file.clone(),
        r#"42
    "#,
    );

    cmd.arg("--quiet")
        .env("VECTOR_CONFIG_DIR", config_dir)
        .env("VECTOR_DATA_DIR", create_directory())
        .env("VECTOR_TEST_INPUT_FILE", input_file.clone());

    // Run vector
    let vector = cmd.stdout(std::process::Stdio::piped()).spawn().unwrap();

    // Give vector time to start.
    sleep(STARTUP_TIME);

    // Signal shutdown
    kill(Pid::from_raw(vector.id() as i32), Signal::SIGTERM).unwrap();

    // Wait for shutdown
    let output = vector.wait_with_output().unwrap();
    assert!(output.status.success(), "Vector didn't exit successfully.");

    // Output
    let event: Value = serde_json::from_slice(output.stdout.as_slice()).unwrap();
    assert_eq!(event["message"], json!("42"));
    assert_eq!(event["test_host"], json!("runner"));
}

#[test]
fn configuration_path_recomputed() {
    // Directory with configuration files
    let dir = create_directory();

    // First configuration file
    overwrite_file(
        dir.join("conf1.toml"),
        &source_config(
            r#"
        type = "demo_logs"
        format = "shuffle"
        interval = 1.0 # optional, no default
        lines = ["foo", "bar"]"#,
        ),
    );

    // Vector command
    let mut cmd = vector_with(dir.join("*"), next_addr(), true);

    // Run vector
    let mut vector = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // Give vector time to start.
    sleep(STARTUP_TIME);

    // Second configuration file
    overwrite_file(dir.join("conf2.toml"), STDIO_CONFIG);
    // Clean the first file so to have only the console source.
    overwrite_file(dir.join("conf1.toml"), &"");

    // Signal reload
    kill(Pid::from_raw(vector.id() as i32), Signal::SIGHUP).unwrap();

    // Message to assert, sended to console source and picked up from
    // console sink, both added in the second configuration file.
    vector
        .stdin
        .as_mut()
        .unwrap()
        .write_all("42".as_bytes())
        .unwrap();

    // Wait for shutdown
    // Test will hang here if the other config isn't picked up.
    let output = vector.wait_with_output().unwrap();
    assert!(output.status.success(), "Vector didn't exit successfully.");

    // Output
    assert_eq!(output.stdout.as_slice(), "42\n".as_bytes());
}

#[test]
fn remove_unix_socket_stream() {
    let dir = create_directory();
    let mut path = dir.clone();
    path.push("tmp");
    path.set_extension("sock");

    test_timely_shutdown(source_vector(&format!(
        r#"
        type = "socket"
        path = "{}"
        mode = "unix"
        "#,
        path.to_string_lossy()
    )));

    // Assert that data folder is empty
    assert!(read_dir(dir).unwrap().next().is_none());
}

#[test]
fn remove_unix_socket_datagram() {
    let dir = create_directory();
    let mut path = dir.clone();
    path.push("tmp");
    path.set_extension("sock");

    test_timely_shutdown(source_vector(&format!(
        r#"
        type = "socket"
        path = "{}"
        mode = "unix_datagram"
        "#,
        path.to_string_lossy()
    )));

    // Assert that data folder is empty
    assert!(read_dir(dir).unwrap().next().is_none());
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
fn timely_shutdown_demo_logs() {
    test_timely_shutdown(source_vector(
        r#"
    type = "demo_logs"
    format = "shuffle"
    interval = 1.0 # optional, no default
    lines = ["foo", "bar"]"#,
    ));
}

#[test]
fn timely_shutdown_http() {
    test_timely_shutdown(source_vector(
        r#"
    type = "http"
    address = "${VECTOR_TEST_ADDRESS}"
    decoding.codec = "bytes""#,
    ));
}

#[test]
fn timely_shutdown_heroku_logs() {
    test_timely_shutdown(source_vector(
        r#"
    type = "heroku_logs"
    address = "${VECTOR_TEST_ADDRESS}""#,
    ));
}

#[test]
fn timely_shutdown_docker() {
    test_timely_shutdown(source_vector(r#"type = "docker_logs""#));
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
    test_timely_shutdown_with_sub(
        vector_with(create_file(PROMETHEUS_SINK_CONFIG), address, false),
        |_| {
            test_timely_shutdown(vector_with(
                create_file(
                    source_config(
                        r#"
        type = "prometheus_scrape"
        hosts = ["http://${VECTOR_TEST_ADDRESS}"]"#,
                    )
                    .as_str(),
                ),
                address,
                false,
            ));
        },
    );
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
    vector::test_util::trace_init();
    test_timely_shutdown(source_vector(
        r#"
    type = "splunk_hec"
    address = "${VECTOR_TEST_ADDRESS}""#,
    ));
}

#[test]
fn timely_shutdown_statsd() {
    vector::test_util::trace_init();
    test_timely_shutdown(source_vector(
        r#"
    type = "statsd"
    mode = "tcp"
    address = "${VECTOR_TEST_ADDRESS}""#,
    ));
}

#[test]
fn timely_shutdown_syslog_tcp() {
    vector::test_util::trace_init();
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
fn timely_shutdown_vector_v2() {
    test_timely_shutdown(source_vector(
        r#"
    type = "vector"
    version = "2"
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
  encoding.codec = "text"
  target = "stdout"
"#,
    ));
}

#[test]
fn timely_reload_shutdown() {
    let path = create_file(
        source_config(
            r#"
            type = "socket"
            address = "${VECTOR_TEST_ADDRESS}"
            mode = "tcp""#,
        )
        .as_str(),
    );

    let mut cmd = vector_with(path.clone(), next_addr(), false);
    cmd.arg("-w");

    test_timely_shutdown_with_sub(cmd, |vector| {
        overwrite_file(
            path,
            source_config(
                r#"
                type = "socket"
                address = "${VECTOR_TEST_ADDRESS}"
                mode = "udp""#,
            )
            .as_str(),
        );

        // Give vector time to reload.
        sleep(RELOAD_TIME);

        // Check if vector is still running
        assert_eq!(
            None,
            vector.try_wait().unwrap(),
            "Vector exited too early on reload."
        );
    });
}

#[tokio::test]
async fn health_503_during_shutdown() {
    use std::process::Command;

    let mut cmd = Command::cargo_bin("vector").unwrap();

    cmd.arg("--quiet")
        .arg("-c")
        .arg(create_file(
            r#"
            [api]
              enabled = true
              address = "127.0.0.1:8686"

            [sources.source]
              type = "demo_logs"
              format = "json"
              interval = 0

            [sinks.sink]
              type = "blackhole"
              inputs = ["source"]
              rate = 1
            "#,
        ))
        .env("VECTOR_DATA_DIR", create_directory());

    let mut vector = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // Give vector time to start.
    sleep(STARTUP_TIME);

    // Check if vector is still running
    assert_eq!(None, vector.try_wait().unwrap(), "Vector exited too early.");

    // Signal shutdown
    kill(Pid::from_raw(vector.id() as i32), Signal::SIGTERM).unwrap();

    // Give vector time to begin shutting down.
    sleep(Duration::from_secs(1));

    let response = reqwest::get("http://127.0.0.1:8686/health").await.unwrap();

    assert_eq!(response.status(), http::StatusCode::SERVICE_UNAVAILABLE);

    kill(Pid::from_raw(vector.id() as i32), Signal::SIGKILL).unwrap();
}
