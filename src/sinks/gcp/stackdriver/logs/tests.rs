//! Unit tests for the `gcp_stackdriver_logs` sink.

use std::collections::HashMap;

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{future::ready, stream};
use http::Uri;
use indoc::indoc;
use serde::Deserialize;
use vector_lib::lookup::lookup_v2::ConfigValuePath;
use vrl::{event_path, value};

use super::{
    config::{StackdriverConfig, StackdriverResource, default_endpoint},
    encoder::StackdriverLogsEncoder,
};
use crate::{
    config::{GenerateConfig, SinkConfig, SinkContext},
    event::{LogEvent, Value},
    gcp::GcpAuthenticator,
    sinks::{
        gcp::stackdriver::logs::{
            config::{StackdriverLabelConfig, StackdriverLogName},
            encoder::remap_severity,
            service::StackdriverLogsServiceRequestBuilder,
        },
        prelude::*,
        util::{
            encoding::Encoder as _,
            http::{HttpRequest, HttpServiceRequestBuilder},
        },
    },
    test_util::{
        components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        http::{always_200_response, spawn_blackhole_http_server},
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<StackdriverConfig>();
}

#[tokio::test]
async fn component_spec_compliance() {
    let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

    let config = StackdriverConfig::generate_config().to_string();
    let mut config = StackdriverConfig::deserialize(
        toml::de::ValueDeserializer::parse(&config).expect("toml should deserialize"),
    )
    .expect("config should be valid");

    // If we don't override the credentials path/API key, it tries to directly call out to the Google Instance
    // Metadata API, which we clearly don't have in unit tests. :)
    config.auth.credentials_path = None;
    config.auth.api_key = Some("fake".to_string().into());
    config.endpoint = mock_endpoint.to_string();

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();

    let event = Event::Log(LogEvent::from("simple message"));
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;
}

#[test]
fn encode_valid() {
    let mut transformer = Transformer::default();
    transformer
        .set_except_fields(Some(vec![
            "anumber".into(),
            "node_id".into(),
            "log_id".into(),
        ]))
        .unwrap();

    let encoder = StackdriverLogsEncoder::new(
        transformer,
        Template::try_from("{{ log_id }}").unwrap(),
        StackdriverLogName::Project("project".to_owned()),
        StackdriverLabelConfig {
            labels_key: None,
            labels: HashMap::from([(
                "config_user_label_1".to_owned(),
                Template::try_from("config_user_value_1").unwrap(),
            )]),
        },
        StackdriverResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([
                (
                    "namespace".to_owned(),
                    Template::try_from("office").unwrap(),
                ),
                (
                    "node_id".to_owned(),
                    Template::try_from("{{ node_id }}").unwrap(),
                ),
            ]),
        },
        Some(ConfigValuePath::try_from("anumber".to_owned()).unwrap()),
    );

    let mut log = [
        ("message", "hello world"),
        ("anumber", "100"),
        ("node_id", "10.10.10.1"),
        ("log_id", "testlogs"),
    ]
    .iter()
    .copied()
    .collect::<LogEvent>();
    log.insert(
        event_path!("logging.googleapis.com/labels"),
        value!({user_label_1: "user_value_1"}),
    );

    let json = encoder.encode_event(Event::from(log)).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "logName":"projects/project/logs/testlogs",
            "jsonPayload":{"message":"hello world"},
            "severity":100,
            "labels":{
                "config_user_label_1":"config_user_value_1",
                "user_label_1":"user_value_1"
            },
            "resource":{
                "type":"generic_node",
                "labels":{"namespace":"office","node_id":"10.10.10.1"}
            }
        })
    );
}

#[test]
fn encode_inserts_timestamp() {
    let transformer = Transformer::default();

    let encoder = StackdriverLogsEncoder::new(
        transformer,
        Template::try_from("testlogs").unwrap(),
        StackdriverLogName::Project("project".to_owned()),
        StackdriverLabelConfig {
            labels_key: None,
            labels: HashMap::from([(
                "config_user_label_1".to_owned(),
                Template::try_from("value_1").unwrap(),
            )]),
        },
        StackdriverResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([(
                "namespace".to_owned(),
                Template::try_from("office").unwrap(),
            )]),
        },
        Some(ConfigValuePath::try_from("anumber".to_owned()).unwrap()),
    );

    let mut log = LogEvent::default();
    log.insert("message", Value::Bytes("hello world".into()));
    log.insert("anumber", Value::Bytes("100".into()));
    log.insert(
        "timestamp",
        Value::Timestamp(
            Utc.with_ymd_and_hms(2020, 1, 1, 12, 30, 0)
                .single()
                .expect("invalid timestamp"),
        ),
    );

    let json = encoder.encode_event(Event::from(log)).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "logName":"projects/project/logs/testlogs",
            "jsonPayload":{"message":"hello world","timestamp":"2020-01-01T12:30:00Z"},
            "severity":100,
            "labels":{"config_user_label_1":"value_1"},
            "resource":{
                "type":"generic_node",
                "labels":{"namespace":"office"}},
            "timestamp":"2020-01-01T12:30:00Z"
        })
    );
}

#[test]
fn severity_remaps_strings() {
    for &(s, n) in &[
        ("EMERGENCY", 800), // Handles full upper case
        ("EMERG", 800),     // Handles abbreviations
        ("FATAL", 800),     // Handles highest alternate
        ("alert", 700),     // Handles lower case
        ("CrIt1c", 600),    // Handles mixed case and suffixes
        ("err404", 500),    // Handles lower case and suffixes
        ("warnings", 400),
        ("notice", 300),
        ("info", 200),
        ("DEBUG2", 100), // Handles upper case and suffixes
        ("trace", 100),  // Handles lowest alternate
        ("nothing", 0),  // Maps unknown terms to DEFAULT
        ("123", 100),    // Handles numbers in strings
        ("-100", 0),     // Maps negatives to DEFAULT
    ] {
        assert_eq!(
            remap_severity(s.into()),
            Value::Integer(n),
            "remap_severity({s:?}) != {n}"
        );
    }
}

#[tokio::test]
async fn correct_request() {
    let uri: Uri = default_endpoint().parse().unwrap();

    let transformer = Transformer::default();
    let encoder = StackdriverLogsEncoder::new(
        transformer,
        Template::try_from("testlogs").unwrap(),
        StackdriverLogName::Project("project".to_owned()),
        StackdriverLabelConfig {
            labels_key: None,
            labels: HashMap::from([(
                "config_user_label_1".to_owned(),
                Template::try_from("value_1").unwrap(),
            )]),
        },
        StackdriverResource {
            type_: "generic_node".to_owned(),
            labels: HashMap::from([(
                "namespace".to_owned(),
                Template::try_from("office").unwrap(),
            )]),
        },
        None,
    );

    let log1 = [("message", "hello")].iter().copied().collect::<LogEvent>();
    let log2 = [("message", "world")].iter().copied().collect::<LogEvent>();

    let events = vec![Event::from(log1), Event::from(log2)];

    let mut writer = Vec::new();
    let (_, _) = encoder.encode_input(events, &mut writer).unwrap();

    let body = Bytes::copy_from_slice(&writer);

    let stackdriver_logs_service_request_builder = StackdriverLogsServiceRequestBuilder {
        uri: uri.clone(),
        auth: GcpAuthenticator::None,
    };

    let http_request = HttpRequest::new(
        body,
        EventFinalizers::default(),
        RequestMetadata::default(),
        (),
    );

    let request = stackdriver_logs_service_request_builder
        .build(http_request)
        .unwrap();

    let (parts, body) = request.into_parts();
    let json: serde_json::Value = serde_json::from_slice(&body[..]).unwrap();

    assert_eq!(
        &parts.uri.to_string(),
        "https://logging.googleapis.com/v2/entries:write"
    );
    assert_eq!(
        json,
        serde_json::json!({
            "entries": [
                {
                    "logName": "projects/project/logs/testlogs",
                    "severity": 0,
                    "jsonPayload": {
                        "message": "hello"
                    },
                    "labels": {
                        "config_user_label_1": "value_1"
                    },
                    "resource": {
                        "type": "generic_node",
                        "labels": {
                            "namespace": "office"
                        }
                    }
                },
                {
                    "logName": "projects/project/logs/testlogs",
                    "severity": 0,
                    "jsonPayload": {
                        "message": "world"
                    },
                    "labels": {
                        "config_user_label_1": "value_1"
                    },
                    "resource": {
                        "type": "generic_node",
                        "labels": {
                            "namespace": "office"
                        }
                    }
                }
            ]
        })
    );
}

#[tokio::test]
async fn fails_missing_creds() {
    let config: StackdriverConfig = toml::from_str(indoc! {r#"
            project_id = "project"
            log_id = "testlogs"
            resource.type = "generic_node"
            resource.namespace = "office"
        "#})
    .unwrap();
    if config.build(SinkContext::default()).await.is_ok() {
        panic!("config.build failed to error");
    }
}

#[test]
fn fails_invalid_log_names() {
    toml::from_str::<StackdriverConfig>(indoc! {r#"
            log_id = "testlogs"
            resource.type = "generic_node"
            resource.namespace = "office"
        "#})
    .expect_err("Config parsing failed to error with missing ids");

    toml::from_str::<StackdriverConfig>(indoc! {r#"
            project_id = "project"
            folder_id = "folder"
            log_id = "testlogs"
            resource.type = "generic_node"
            resource.namespace = "office"
        "#})
    .expect_err("Config parsing failed to error with extraneous ids");
}

/// Tests that exercise the sink's behaviour when the downstream HTTP backend
/// is reachable at the TCP level but never returns a response — the scenario
/// that caused the pod deadlock in production.
mod sink_hang_tests {
    use std::{convert::Infallible, num::NonZeroU64, time::Duration};

    use futures::{future::ready, stream};
    use http::{Request, Response};
    use hyper::Body;
    use serde::Deserialize;
    use vector_lib::finalization::{BatchNotifier, BatchStatus};

    use crate::{
        config::{GenerateConfig, SinkConfig, SinkContext},
        event::LogEvent,
        sinks::gcp::stackdriver::logs::config::StackdriverConfig,
        test_util::http::spawn_blackhole_http_server,
    };

    /// HTTP handler that accepts the connection but never sends a response.
    ///
    /// Models GCP's LB being reachable at the TCP/HTTP level (so H2 keepalive
    /// PINGs are answered and the connection stays ESTABLISHED) while the backend
    /// has stopped processing requests.  Every request hangs here until Tower's
    /// `Timeout` layer fires.
    async fn never_respond(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
        std::future::pending().await
    }

    /// Build a StackdriverConfig pointed at `endpoint` with test-friendly timeouts.
    fn make_config(endpoint: &str, timeout_secs: u64, retry_attempts: usize) -> StackdriverConfig {
        let raw = StackdriverConfig::generate_config().to_string();
        let mut cfg = StackdriverConfig::deserialize(
            toml::de::ValueDeserializer::parse(&raw).expect("toml should deserialize"),
        )
        .expect("config should be valid");
        cfg.auth.credentials_path = None;
        cfg.auth.api_key = Some("fake-key".to_string().into());
        cfg.endpoint = endpoint.to_string();
        cfg.request.timeout_secs = timeout_secs;
        cfg.request.retry_attempts = retry_attempts;
        cfg.request.retry_initial_backoff_secs = NonZeroU64::new(1).unwrap();
        cfg.request.retry_max_duration_secs = NonZeroU64::new(2).unwrap();
        cfg
    }

    /// End-to-end regression test for the production deadlock.
    ///
    /// A `never_respond` HTTP server simulates GCP stopping request processing
    /// while keeping the connection open.  With `timeout_secs = 2` and
    /// `retry_attempts = 2` the full cycle is 3 attempts × 2 s + 2 × 1 s
    /// backoff ≈ 8 s.
    ///
    /// Asserts:
    /// - The Driver exits (does NOT hang forever).
    /// - The events are finalized as `Failed` after retry exhaustion.
    /// - The elapsed time is consistent with the configured timeouts.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sink_retries_exhaust_and_reject_events_on_hanging_backend() {
        crate::test_util::trace_init();

        let endpoint = spawn_blackhole_http_server(never_respond).await;
        let config = make_config(&endpoint.to_string(), 2, 2);

        let (sink, _healthcheck) = config.build(SinkContext::default()).await.unwrap();

        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let event = LogEvent::from("test message").with_batch_notifier(&batch);
        drop(batch);

        let start = tokio::time::Instant::now();
        sink.run_events(stream::once(ready(event))).await.unwrap();
        let elapsed = start.elapsed();

        // Yield so the finalizer task can propagate the Rejected status.
        tokio::task::yield_now().await;

        let status = receiver.await;
        assert_eq!(
            status,
            BatchStatus::Failed,
            "events should be Rejected after all retries are exhausted"
        );

        // 3 attempts × 2 s timeout + 2 × 1 s Fibonacci backoff = ~8 s.
        // The lower bound catches cases where the timeout isn't firing at all;
        // the upper bound catches regressions where the Driver hangs indefinitely.
        assert!(
            elapsed >= Duration::from_secs(5),
            "retries finished too quickly ({:?}); tower Timeout may not be firing",
            elapsed
        );
        assert!(
            elapsed < Duration::from_secs(30),
            "Driver hung for {:?} — possible regression of the deadlock bug",
            elapsed
        );
    }

    /// Regression test using synthetic time.
    ///
    /// With Tokio's time paused we can drive the full retry + stall-detection
    /// cycle in wall-clock milliseconds.  This verifies that:
    ///
    /// 1. The Driver's new stall-detection `select!` arm (fires every 60 s) does
    ///    not interfere with normal retry processing.
    /// 2. After each Tower `Timeout` fires (at `t = timeout_secs`) the retry
    ///    policy re-submits the request.
    /// 3. Once retries are exhausted the Driver exits and events are `Failed`.
    ///
    /// Uses `start_paused = true` so the test completes in milliseconds rather
    /// than the 90+ seconds that a 30 s timeout × 3 attempts would require.
    #[tokio::test(start_paused = true)]
    async fn sink_driver_completes_with_paused_time_advancing_through_stall_window() {
        crate::test_util::trace_init();

        // 30 s timeout mirrors production; 2 retries keeps the test short.
        let endpoint = spawn_blackhole_http_server(never_respond).await;
        let config = make_config(&endpoint.to_string(), 30, 2);

        let (sink, _healthcheck) = config.build(SinkContext::default()).await.unwrap();

        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let event = LogEvent::from("stall test").with_batch_notifier(&batch);
        drop(batch);

        // Run the sink in a background task; drive it by advancing simulated time.
        let sink_task = tokio::spawn(async move {
            sink.run_events(stream::once(ready(event))).await
        });

        // Yield several times to let the sink task start, establish the TCP
        // connection to the server, and submit the first request before we begin
        // advancing time.
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }

        // ── Attempt 1 ────────────────────────────────────────────────────────
        // Advance past: 30 s timeout + 1 s Fibonacci backoff.
        // The stall-detection interval (60 s) does NOT fire here; it fires later.
        tokio::time::advance(Duration::from_secs(31)).await;
        // Yield to let the retry task re-submit the request on a new connection.
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }

        // ── Attempt 2 + stall-detection window ───────────────────────────────
        // Advance 31 s (timeout) + enough to cross the 60 s stall-check mark.
        // At t ≈ 62 s the stall-detection arm in the Driver fires and emits a
        // warn! — this is the new observability signal added in this patch.
        tokio::time::advance(Duration::from_secs(31)).await;
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }

        // ── Attempt 3 (final) ────────────────────────────────────────────────
        // Advance past the final timeout.  Retries are now exhausted; the Driver
        // should return and finalize the event as Failed.
        tokio::time::advance(Duration::from_secs(31)).await;
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }

        // The sink task must have completed — if it hasn't, the Driver deadlocked.
        // Note: do NOT wrap in tokio::time::timeout here because time is still paused;
        // a timer-based timeout would never fire.  If there is a regression the test will
        // simply hang, which is the correct failure signal for a deadlock bug.
        let result = sink_task.await.expect("sink task panicked");
        assert!(result.is_ok(), "Driver returned Err unexpectedly");

        tokio::task::yield_now().await;

        let status = receiver.await;
        assert_eq!(
            status,
            BatchStatus::Failed,
            "events should be Rejected after retry exhaustion (paused-time run)"
        );
    }
}
