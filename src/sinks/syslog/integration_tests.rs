use std::{
    future::ready,
    path::PathBuf,
    time::{Duration, Instant},
};

use futures::stream;
use tokio::time::sleep;
use vector_lib::event::{Event, LogEvent, ObjectMap, Value};

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
    log.insert("host", "vector-integration-host");
    log.insert("app", "vector-integration-app");
    log.insert("facility", facility);
    log.insert("severity", severity);
    event
}

async fn run_sink(config: SyslogSinkConfig, event: Event) {
    // Build the sink inside `assert_sink_compliance` so the registered-event
    // names from `register!(BytesSent::from(Protocol::UDP))` are captured
    // after `init_test` clears the event recorder.
    assert_sink_compliance(&SINK_TAGS, async move {
        let context = SinkContext::default();
        let (sink, healthcheck) = config.build(context).await.expect("sink should build");
        healthcheck.await.expect("healthcheck should pass");
        sink.run(stream::once(ready(event.into())))
            .await
            .expect("sink should run");
    })
    .await;
}

async fn wait_for_log_contains(file_name: &str, needle: &str) -> String {
    let path = syslog_log_dir().join(file_name);
    let started = Instant::now();
    let mut contents = String::new();

    while started.elapsed() <= Duration::from_secs(15) {
        contents = std::fs::read_to_string(&path).unwrap_or_default();
        if contents.contains(needle) {
            return contents;
        }

        sleep(Duration::from_millis(100)).await;
    }

    panic!(
        "timed out waiting for {needle:?} in {}; contents:\n{contents}",
        path.display()
    );
}

fn assert_received_syslog(contents: &str, pri_prefix: &str, message: &str) {
    assert!(
        contents.contains(pri_prefix),
        "expected PRI prefix {pri_prefix:?} in rsyslog output:\n{contents}"
    );
    assert!(
        contents.contains("vector-integration-host"),
        "expected host in rsyslog output:\n{contents}"
    );
    assert!(
        contents.contains("vector-integration-app"),
        "expected app name in rsyslog output:\n{contents}"
    );
    assert!(
        contents.contains(message),
        "expected message {message:?} in rsyslog output:\n{contents}"
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

    let contents = wait_for_log_contains("udp.log", &message).await;
    assert_received_syslog(&contents, "<133>", &message);
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

    let contents = wait_for_log_contains("udp.log", &message).await;
    assert_received_syslog(&contents, "<134>1", &message);
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

    let contents = wait_for_log_contains("tcp-line.log", &message).await;
    assert_received_syslog(&contents, "<140>", &message);
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

    let contents = wait_for_log_contains("tcp-line.log", &message).await;
    assert_received_syslog(&contents, "<139>1", &message);
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

    let contents = wait_for_log_contains("tcp-octet.log", &message).await;
    assert_received_syslog(&contents, "<146>1", &message);
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
    log.insert("pid", proc_id.as_str());
    log.insert("mid", msg_id.as_str());
    let mut sd_params = ObjectMap::new();
    sd_params.insert("retry".into(), Value::from(sd_param_value.as_str()));
    let mut sd_root = ObjectMap::new();
    sd_root.insert("metrics@1234".into(), Value::from(sd_params));
    log.insert("structured_data", Value::from(sd_root));

    run_sink(config, event).await;

    let contents = wait_for_log_contains("tcp-octet.log", &message).await;
    assert_received_syslog(&contents, "<150>1", &message);
    assert!(
        contents.contains(&proc_id),
        "expected proc_id {proc_id:?} in rsyslog output:\n{contents}"
    );
    assert!(
        contents.contains(&msg_id),
        "expected msg_id {msg_id:?} in rsyslog output:\n{contents}"
    );
    let sd_fragment = format!("[metrics@1234 retry=\"{sd_param_value}\"]");
    assert!(
        contents.contains(&sd_fragment),
        "expected structured-data element {sd_fragment:?} in rsyslog output:\n{contents}"
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

    let contents = wait_for_log_contains("tcp-octet.log", &second_line).await;
    assert_received_syslog(&contents, "<146>1", &first_line);
    assert!(
        contents.contains(&second_line),
        "expected multiline message tail in rsyslog output:\n{contents}"
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

    let contents = wait_for_log_contains("syslog-ng-tcp-tls.log", &message).await;
    assert_received_syslog(&contents, "<156>1", &message);
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

    let contents = wait_for_log_contains("syslog-ng-udp.log", &message).await;
    assert_received_syslog(&contents, "<166>1", &message);
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

    let contents = wait_for_log_contains("syslog-ng-tcp-octet.log", &message).await;
    assert_received_syslog(&contents, "<173>1", &message);
}
