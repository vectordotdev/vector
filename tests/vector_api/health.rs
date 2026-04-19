//! Integration tests for the standard gRPC health check on the observability API.

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
