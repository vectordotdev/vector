//! Integration tests for http_scrape source.
//! The container configuration file is `docker-compose-.http_scrape.yml`
//! It leverages a static file server which serves the files in tests/data/http-scrape

use tokio::time::{Duration, Instant};

use crate::{
    config::{ComponentKey, SourceConfig, SourceContext},
    http::Auth,
    serde::default_framing_message_based,
    sources::http_scrape::scrape::NAME,
    tls,
    tls::TlsConfig,
    SourceSender,
};
use codecs::decoding::DeserializerConfig;
use vector_core::config::log_schema;

use super::{
    tests::{run_compliance, run_error, INTERVAL_SECS},
    HttpScrapeConfig,
};

/// Logs (raw bytes) should be scraped and decoded successfully.
#[tokio::test]
async fn scraped_logs_bytes() {
    let events = run_compliance(HttpScrapeConfig::new(
        "http://dufs:5000/logs/bytes".to_string(),
        INTERVAL_SECS,
        None,
        DeserializerConfig::Bytes,
        default_framing_message_based(),
        None,
        None,
        None,
    ))
    .await;
    let log = events[0].as_log();
    assert_eq!(log[log_schema().source_type_key()], NAME.into());
}

/// Logs (json) should be scraped and decoded successfully.
#[tokio::test]
async fn scraped_logs_json() {
    let events = run_compliance(HttpScrapeConfig::new(
        "http://dufs:5000/logs/json.json".to_string(),
        INTERVAL_SECS,
        None,
        DeserializerConfig::Json,
        default_framing_message_based(),
        None,
        None,
        None,
    ))
    .await;
    let log = events[0].as_log();
    assert_eq!(log[log_schema().source_type_key()], NAME.into());
}

/// Metrics should be scraped and decoded successfully.
#[tokio::test]
async fn scraped_metrics_native_json() {
    let events = run_compliance(HttpScrapeConfig::new(
        "http://dufs:5000/metrics/native.json".to_string(),
        INTERVAL_SECS,
        None,
        DeserializerConfig::NativeJson,
        default_framing_message_based(),
        None,
        None,
        None,
    ))
    .await;

    let metric = events[0].as_metric();
    assert_eq!(
        metric.tags().unwrap()[log_schema().source_type_key()],
        NAME.to_string()
    );
}

/// Traces should be scraped and decoded successfully.
#[tokio::test]
async fn scraped_trace_native_json() {
    let events = run_compliance(HttpScrapeConfig::new(
        "http://dufs:5000/traces/native.json".to_string(),
        INTERVAL_SECS,
        None,
        DeserializerConfig::NativeJson,
        default_framing_message_based(),
        None,
        None,
        None,
    ))
    .await;

    let trace = events[0].as_trace();
    assert_eq!(trace.as_map()[log_schema().source_type_key()], NAME.into());
}

/// Passing no authentication for the auth-gated endpoint should yield errors.
#[tokio::test]
async fn unauthorized_no_auth() {
    run_error(HttpScrapeConfig::new(
        "http://dufs-auth:5000/logs/json.json".to_string(),
        INTERVAL_SECS,
        None,
        DeserializerConfig::Json,
        default_framing_message_based(),
        None,
        None,
        None,
    ))
    .await;
}

/// Passing the incorrect credentials for the auth-gated endpoint should yield errors.
#[tokio::test]
async fn unauthorized_wrong_auth() {
    run_error(HttpScrapeConfig::new(
        "http://dufs-auth:5000/logs/json.json".to_string(),
        INTERVAL_SECS,
        None,
        DeserializerConfig::Json,
        default_framing_message_based(),
        None,
        None,
        Some(Auth::Basic {
            user: "white_rabbit".to_string(),
            password: "morpheus".to_string(),
        }),
    ))
    .await;
}

/// Passing the correct credentials for the auth-gated endpoint should succeed.
#[tokio::test]
async fn authorized() {
    run_compliance(HttpScrapeConfig::new(
        "http://dufs-auth:5000/logs/json.json".to_string(),
        INTERVAL_SECS,
        None,
        DeserializerConfig::Json,
        default_framing_message_based(),
        None,
        None,
        Some(Auth::Basic {
            user: "user".to_string(),
            password: "pass".to_string(),
        }),
    ))
    .await;
}

/// Passing the CA file for TLS should yield errors.
#[tokio::test]
async fn tls_invalid_ca() {
    run_compliance(HttpScrapeConfig::new(
        "https://dufs-https:5000/logs/json.json".to_string(),
        INTERVAL_SECS,
        None,
        DeserializerConfig::Json,
        default_framing_message_based(),
        None,
        Some(TlsConfig {
            ca_file: Some(tls::TEST_PEM_INTERMEDIATE_CA_PATH.into()),
            ..Default::default()
        }),
        None,
    ))
    .await;
}

/// Passing the correct CA file for TLS should succeed.
#[tokio::test]
async fn tls_valid() {
    run_compliance(HttpScrapeConfig::new(
        "https://dufs-https:5000/logs/json.json".to_string(),
        INTERVAL_SECS,
        None,
        DeserializerConfig::Json,
        default_framing_message_based(),
        None,
        Some(TlsConfig {
            ca_file: Some(tls::TEST_PEM_CA_PATH.into()),
            ..Default::default()
        }),
        None,
    ))
    .await;
}

/// The source should shutdown cleanly when the shutdown signal is received.
/// TODO this can probably be extracted into the test_utils and generalized for other sources to
/// use.
#[tokio::test]
async fn shutdown() {
    let source_id = ComponentKey::from("http_scrape_shutdown");
    let source = HttpScrapeConfig::new(
        "http://dufs:5000/logs/json.json".to_string(),
        INTERVAL_SECS,
        None,
        DeserializerConfig::Json,
        default_framing_message_based(),
        None,
        None,
        None,
    );

    // build the context for the source and get a SourceShutdownCoordinator to signal with
    let (tx, _rx) = SourceSender::new_test();
    let (context, mut shutdown) = SourceContext::new_shutdown(&source_id, tx);

    // start source
    let source = source
        .build(context)
        .await
        .expect("source should not fail to build");
    let source_handle = tokio::spawn(source);

    // signal the source to shut down
    let deadline = Instant::now() + Duration::from_secs(1);
    let shutdown_complete = shutdown.shutdown_source(&source_id, deadline);
    let shutdown_success = shutdown_complete.await;
    assert!(shutdown_success);

    // Ensure source actually shut down successfully.
    let _ = source_handle.await.unwrap();
}
