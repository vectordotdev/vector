//! Integration tests for the health endpoints on the observability API:
//!
//! * the standard gRPC health check (`grpc.health.v1.Health/Check`)
//! * the HTTP `/health` endpoint served on the same port

use super::{common::*, harness::*};

// ============================================================================
// Tests
// ============================================================================

/// Verifies that the standard gRPC health check (empty service = whole-server
/// health) reports SERVING on a running Vector instance.
#[tokio::test]
async fn health_check_reports_serving() {
    let config = single_source_config("demo", 1.0, None);
    let mut harness = TestHarness::new(&config)
        .await
        .expect("Vector should start");

    harness
        .api_client()
        .health()
        .await
        .expect("health check should report SERVING");

    assert!(harness.check_running(), "Vector should still be running");
}

/// Verifies the HTTP `GET /health` endpoint returns 200 with `{"ok":true}` on a
/// running Vector instance. This endpoint is load-balancer friendly and is
/// shared with gRPC clients on the same API port.
#[tokio::test]
async fn http_health_endpoint_returns_200_when_serving() {
    let config = single_source_config("demo", 1.0, None);
    let harness = TestHarness::new(&config)
        .await
        .expect("Vector should start");

    let url = format!("http://127.0.0.1:{}/health", harness.api_port());
    let response = reqwest::get(&url)
        .await
        .expect("GET /health should succeed");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.expect("body should be readable");
    assert_eq!(body, r#"{"ok":true}"#);
}

/// Verifies HTTP `HEAD /health` returns 200 without a body. ALB/ELB style
/// probes that prefer HEAD should work the same as GET.
#[tokio::test]
async fn http_health_endpoint_supports_head() {
    let config = single_source_config("demo", 1.0, None);
    let harness = TestHarness::new(&config)
        .await
        .expect("Vector should start");

    let url = format!("http://127.0.0.1:{}/health", harness.api_port());
    let response = reqwest::Client::new()
        .head(&url)
        .send()
        .await
        .expect("HEAD /health should succeed");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
}
