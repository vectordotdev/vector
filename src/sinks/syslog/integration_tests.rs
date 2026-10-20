use std::{
    future::ready,
    path::PathBuf,
    time::{Duration, Instant},
};

use futures::stream;
use tokio::time::sleep;
use vector_lib::event::{Event, LogEvent, ObjectMap, Value};
use vrl::event_path;

use super::SyslogSinkConfig;
use crate::{
    config::{SinkConfig, SinkContext},
    test_util::{
        components::{SINK_TAGS, assert_sink_compliance},
        random_string, trace_init, wait_for_tcp,
    },
    tls::{self, TlsConfig, TlsEnableableConfig},
};

// These tests exercise receiver-visible interoperability with a real rsyslog
// instance. Unit tests cover exact wire bytes; integration tests stay focused on
// common deployment shapes that operators are likely to run.
const TCP_LINE_DEFAULT: &str = "rsyslog:5515";

fn syslog_udp_address() -> String {
    std::env::var("SYSLOG_UDP_ADDRESS").unwrap_or_else(|_| "rsyslog:5514".to_owned())
}

fn syslog_tcp_line_address() -> String {
    std::env::var("SYSLOG_TCP_LINE_ADDRESS").unwrap_or_else(|_| TCP_LINE_DEFAULT.to_owned())
}

fn syslog_tcp_octet_address() -> String {
    std::env::var("SYSLOG_TCP_OCTET_ADDRESS").unwrap_or_else(|_| "rsyslog:5516".to_owned())
}

fn syslog_ng_tcp_tls_address() -> String {
    std::env::var("SYSLOG_NG_TCP_TLS_ADDRESS").unwrap_or_else(|_| "syslog-ng:5517".to_owned())
}

fn syslog_ng_udp_address() -> String {
    std::env::var("SYSLOG_NG_UDP_ADDRESS").unwrap_or_else(|_| "syslog-ng:5514".to_owned())
}

fn syslog_ng_tcp_line_address() -> String {
    std::env::var("SYSLOG_NG_TCP_LINE_ADDRESS").unwrap_or_else(|_| "syslog-ng:5515".to_owned())
}

fn syslog_ng_tcp_octet_address() -> String {
    std::env::var("SYSLOG_NG_TCP_OCTET_ADDRESS").unwrap_or_else(|_| "syslog-ng:5516".to_owned())
}

fn syslog_log_dir() -> PathBuf {
    std::env::var_os("SYSLOG_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/syslog_logs"))
}

fn log_event(message: &str, facility: &str, severity: &str) -> Event {
    let mut event = Event::Log(LogEvent::from(message.to_owned()));
    let log = event.as_mut_log();
    log.insert(event_path!("host"), "vector-integration-host");
    log.insert(event_path!("app"), "vector-integration-app");
    log.insert(event_path!("facility"), facility);
    log.insert(event_path!("severity"), severity);
    event
}

async fn run_sink(config: SyslogSinkConfig, event: Event) {
    // Build the sink inside `assert_sink_compliance` so the registered-event
    // names from `register!(BytesSent::from(Protocol::UDP))` are captured
    // after `init_test` clears the event recorder.
    assert_sink_compliance(&SINK_TAGS, async move {
        let context = SinkContext::default();
        let (sink, healthcheck) = SinkConfig::build(&config, context)
            .await
            .expect("sink should build");
        healthcheck.await.expect("healthcheck should pass");
        sink.run(stream::once(ready(event.into())))
            .await
            .expect("sink should run");
    })
    .await;
}

/// Fields a receiver parsed out of one syslog message, as written by the JSON
/// templates in `tests/integration/syslog/data`.
#[derive(Debug)]
struct ParsedRecord(serde_json::Value);

impl ParsedRecord {
    /// Returns a field as a string. syslog-ng may emit numeric macros as JSON
    /// numbers, so numbers are rendered without quotes; missing fields are empty.
    fn field(&self, name: &str) -> String {
        match self.0.get(name) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        }
    }

    /// The parsed MSG part. rsyslog keeps the space that follows an RFC 3164
    /// TAG in `msg`, so leading whitespace is not significant here.
    fn message(&self) -> String {
        self.field("message").trim_start().to_owned()
    }
}

/// Waits for a receiver to log a message whose parsed MSG contains `needle`.
///
/// The receiver log files are shared between tests (and across local runs),
/// so every test uses a unique message and matches only its own record.
async fn wait_for_record(file_name: &str, needle: &str) -> ParsedRecord {
    let path = syslog_log_dir().join(file_name);
    let started = Instant::now();
    let mut contents = String::new();

    while started.elapsed() <= Duration::from_secs(15) {
        contents = std::fs::read_to_string(&path).unwrap_or_default();
        let record = contents
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .map(ParsedRecord)
            .find(|record| record.message().contains(needle));
        if let Some(record) = record {
            return record;
        }

        sleep(Duration::from_millis(100)).await;
    }

    panic!(
        "timed out waiting for {needle:?} in {}; contents:\n{contents}",
        path.display()
    );
}

/// Asserts that the receiver parsed the PRI, HOSTNAME, APP-NAME, and MSG that
/// the sink sent.
fn assert_received_syslog(record: &ParsedRecord, pri: u8, message: &str) {
    assert_received_header(record, pri);
    assert_eq!(record.message(), message, "message mismatch: {record:?}");
}

/// Asserts that the receiver parsed the PRI, HOSTNAME, and APP-NAME that the
/// sink sent.
fn assert_received_header(record: &ParsedRecord, pri: u8) {
    assert_eq!(
        record.field("facility"),
        (pri / 8).to_string(),
        "facility mismatch: {record:?}"
    );
    assert_eq!(
        record.field("severity"),
        (pri % 8).to_string(),
        "severity mismatch: {record:?}"
    );
    assert_eq!(
        record.field("hostname"),
        "vector-integration-host",
        "hostname mismatch: {record:?}"
    );
    assert_eq!(
        record.field("app_name"),
        "vector-integration-app",
        "app name mismatch: {record:?}"
    );
}

async fn wait_for_rsyslog() {
    wait_for_tcp(syslog_tcp_line_address()).await;
}

async fn wait_for_syslog_ng() {
    wait_for_tcp(syslog_ng_tcp_line_address()).await;
}

async fn wait_for_syslog_ng_tls() {
    wait_for_tcp(syslog_ng_tcp_tls_address()).await;
}

#[tokio::test]
async fn udp_rfc3164_reaches_rsyslog() {
    trace_init();
    wait_for_rsyslog().await;

    let message = format!("udp-rfc3164-{}", random_string(12));
    let config: SyslogSinkConfig = toml::from_str(&format!(
        r#"
        mode = "udp"
        address = "{}"
        syslog.rfc = "rfc3164"
        syslog.app_name = ".app"
        syslog.facility = ".facility"
        syslog.severity = ".severity"
        "#,
        syslog_udp_address(),
    ))
    .expect("config should parse");

    run_sink(config, log_event(&message, "local0", "notice")).await;

    let record = wait_for_record("udp.log", &message).await;
    assert_received_syslog(&record, 133, &message);
}

#[tokio::test]
async fn udp_rfc5424_reaches_rsyslog() {
    trace_init();
    wait_for_rsyslog().await;

    let message = format!("udp-rfc5424-{}", random_string(12));
    let config: SyslogSinkConfig = toml::from_str(&format!(
        r#"
        mode = "udp"
        address = "{}"
        syslog.rfc = "rfc5424"
        syslog.app_name = ".app"
        syslog.facility = ".facility"
        syslog.severity = ".severity"
        "#,
        syslog_udp_address(),
    ))
    .expect("config should parse");

    run_sink(config, log_event(&message, "local0", "info")).await;

    let record = wait_for_record("udp.log", &message).await;
    assert_received_syslog(&record, 134, &message);
}

#[tokio::test]
async fn tcp_newline_rfc3164_reaches_rsyslog() {
    trace_init();
    wait_for_rsyslog().await;

    let message = format!("tcp-newline-rfc3164-{}", random_string(12));
    let config: SyslogSinkConfig = toml::from_str(&format!(
        r#"
        mode = "tcp"
        address = "{}"
        syslog.rfc = "rfc3164"
        syslog.app_name = ".app"
        syslog.facility = ".facility"
        syslog.severity = ".severity"
        "#,
        syslog_tcp_line_address(),
    ))
    .expect("config should parse");

    run_sink(config, log_event(&message, "local1", "warning")).await;

    let record = wait_for_record("tcp-line.log", &message).await;
    assert_received_syslog(&record, 140, &message);
}

#[tokio::test]
async fn tcp_newline_rfc5424_reaches_rsyslog() {
    trace_init();
    wait_for_rsyslog().await;

    let message = format!("tcp-newline-rfc5424-{}", random_string(12));
    let config: SyslogSinkConfig = toml::from_str(&format!(
        r#"
        mode = "tcp"
        address = "{}"
        syslog.rfc = "rfc5424"
        syslog.app_name = ".app"
        syslog.facility = ".facility"
        syslog.severity = ".severity"
        "#,
        syslog_tcp_line_address(),
    ))
    .expect("config should parse");

    run_sink(config, log_event(&message, "local1", "err")).await;

    let record = wait_for_record("tcp-line.log", &message).await;
    assert_received_syslog(&record, 139, &message);
}

#[tokio::test]
async fn tcp_octet_counting_rfc5424_reaches_rsyslog() {
    trace_init();
    wait_for_rsyslog().await;

    let message = format!("tcp-octet-rfc5424-{}", random_string(12));
    let config: SyslogSinkConfig = toml::from_str(&format!(
        r#"
        mode = "tcp"
        address = "{}"
        framing.method = "octet_counting"
        syslog.rfc = "rfc5424"
        syslog.app_name = ".app"
        syslog.facility = ".facility"
        syslog.severity = ".severity"
        "#,
        syslog_tcp_octet_address(),
    ))
    .expect("config should parse");

    run_sink(config, log_event(&message, "local2", "crit")).await;

    let record = wait_for_record("tcp-octet.log", &message).await;
    assert_received_syslog(&record, 146, &message);
}

/// Verifies that `proc_id`, `msg_id`, and `structured_data` configured at the
/// sink level are routed through the encoder and arrive intact at rsyslog.
/// This catches regressions in the field-path plumbing inside `decant_config`
/// or its inputs, which the codec-only unit tests can't see.
#[tokio::test]
async fn tcp_octet_counting_rfc5424_with_proc_id_msg_id_structured_data_reaches_rsyslog() {
    trace_init();
    wait_for_rsyslog().await;

    let id = random_string(12);
    let message = format!("tcp-octet-fields-{id}");
    let proc_id = format!("pid-{id}");
    let msg_id = format!("msg-{id}");
    let sd_param_value = format!("retry-{id}");

    let config: SyslogSinkConfig = toml::from_str(&format!(
        r#"
        mode = "tcp"
        address = "{}"
        framing.method = "octet_counting"
        syslog.rfc = "rfc5424"
        syslog.app_name = ".app"
        syslog.proc_id = ".pid"
        syslog.msg_id = ".mid"
        syslog.facility = ".facility"
        syslog.severity = ".severity"
        "#,
        syslog_tcp_octet_address(),
    ))
    .expect("config should parse");

    let mut event = log_event(&message, "local2", "info");
    let log = event.as_mut_log();
    log.insert(event_path!("pid"), proc_id.as_str());
    log.insert(event_path!("mid"), msg_id.as_str());
    let mut sd_params = ObjectMap::new();
    sd_params.insert("retry".into(), Value::from(sd_param_value.as_str()));
    let mut sd_root = ObjectMap::new();
    sd_root.insert("metrics@1234".into(), Value::from(sd_params));
    log.insert(event_path!("structured_data"), Value::from(sd_root));

    run_sink(config, event).await;

    let record = wait_for_record("tcp-octet.log", &message).await;
    assert_received_syslog(&record, 150, &message);
    assert_eq!(
        record.field("proc_id"),
        proc_id,
        "proc_id mismatch: {record:?}"
    );
    assert_eq!(
        record.field("msg_id"),
        msg_id,
        "msg_id mismatch: {record:?}"
    );
    assert_eq!(
        record.field("structured_data"),
        format!("[metrics@1234 retry=\"{sd_param_value}\"]"),
        "structured data mismatch: {record:?}"
    );
}

#[tokio::test]
async fn tcp_octet_counting_rfc5424_multiline_reaches_rsyslog() {
    trace_init();
    wait_for_rsyslog().await;

    let id = random_string(12);
    let first_line = format!("tcp-octet-multiline-{id}-first");
    let second_line = format!("tcp-octet-multiline-{id}-second");
    let message = format!("{first_line}\n{second_line}");
    let config: SyslogSinkConfig = toml::from_str(&format!(
        r#"
        mode = "tcp"
        address = "{}"
        framing.method = "octet_counting"
        syslog.rfc = "rfc5424"
        syslog.app_name = ".app"
        syslog.facility = ".facility"
        syslog.severity = ".severity"
        "#,
        syslog_tcp_octet_address(),
    ))
    .expect("config should parse");

    run_sink(config, log_event(&message, "local2", "crit")).await;

    let record = wait_for_record("tcp-octet.log", &first_line).await;
    assert_received_header(&record, 146);
    // rsyslog escapes control characters on receipt by default (LF becomes
    // `#012`), so assert that both lines were parsed as a single message
    // rather than comparing the separator byte.
    let received = record.message();
    assert!(
        received.starts_with(&first_line) && received.ends_with(&second_line),
        "expected both lines in a single parsed message: {record:?}"
    );
}

/// RFC 5425 (syslog over TLS) interop with syslog-ng's TLS network
/// transport using octet-counted framing. Catches regressions in the
/// TLS handshake or framing paths that the in-process TLS unit test
/// can miss because it doesn't speak to a real syslog daemon.
#[tokio::test]
async fn tcp_tls_octet_counting_rfc5424_to_syslog_ng() {
    trace_init();
    wait_for_syslog_ng_tls().await;

    let message = format!("tcp-tls-rfc5424-{}", random_string(12));
    let mut config: SyslogSinkConfig = toml::from_str(&format!(
        r#"
        mode = "tcp"
        address = "{}"
        framing.method = "octet_counting"
        syslog.rfc = "rfc5424"
        syslog.app_name = ".app"
        syslog.facility = ".facility"
        syslog.severity = ".severity"
        "#,
        syslog_ng_tcp_tls_address(),
    ))
    .expect("config should parse");

    if let super::Mode::Tcp(tcp_mode) = &mut config.mode {
        // Trust the project test CA but skip hostname verification, since
        // the syslog-ng cert is for `localhost` not `syslog-ng`.
        tcp_mode.config = crate::sinks::util::tcp::TcpSinkConfig::new(
            syslog_ng_tcp_tls_address(),
            None,
            Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    verify_certificate: Some(true),
                    verify_hostname: Some(false),
                    ca_file: Some(tls::TEST_PEM_INTERMEDIATE_CA_PATH.into()),
                    ..Default::default()
                },
            }),
            None,
        );
    } else {
        panic!("expected TCP mode after config parse");
    }

    run_sink(config, log_event(&message, "local3", "warning")).await;

    let record = wait_for_record("syslog-ng-tcp-tls.log", &message).await;
    assert_received_syslog(&record, 156, &message);
}

#[tokio::test]
async fn udp_rfc5424_reaches_syslog_ng() {
    trace_init();
    wait_for_syslog_ng().await;

    let message = format!("syslog-ng-udp-{}", random_string(12));
    let config: SyslogSinkConfig = toml::from_str(&format!(
        r#"
        mode = "udp"
        address = "{}"
        syslog.rfc = "rfc5424"
        syslog.app_name = ".app"
        syslog.facility = ".facility"
        syslog.severity = ".severity"
        "#,
        syslog_ng_udp_address(),
    ))
    .expect("config should parse");

    run_sink(config, log_event(&message, "local4", "info")).await;

    let record = wait_for_record("syslog-ng-udp.log", &message).await;
    assert_received_syslog(&record, 166, &message);
}

#[tokio::test]
async fn tcp_octet_counting_rfc5424_reaches_syslog_ng() {
    trace_init();
    wait_for_syslog_ng().await;

    let message = format!("syslog-ng-tcp-octet-{}", random_string(12));
    let config: SyslogSinkConfig = toml::from_str(&format!(
        r#"
        mode = "tcp"
        address = "{}"
        framing.method = "octet_counting"
        syslog.rfc = "rfc5424"
        syslog.app_name = ".app"
        syslog.facility = ".facility"
        syslog.severity = ".severity"
        "#,
        syslog_ng_tcp_octet_address(),
    ))
    .expect("config should parse");

    run_sink(config, log_event(&message, "local5", "notice")).await;

    let record = wait_for_record("syslog-ng-tcp-octet.log", &message).await;
    assert_received_syslog(&record, 173, &message);
}
