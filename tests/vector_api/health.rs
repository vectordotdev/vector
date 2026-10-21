//! Integration tests for the health and discovery endpoints on the observability API:
//!
//! * the standard gRPC health check (`grpc.health.v1.Health/Check`)
//! * the HTTP `/health` endpoint served on the same port
//! * gRPC server reflection (`grpc.reflection.v1alpha`) served on the same port

use tonic_reflection::pb::v1alpha::{
    ServerReflectionRequest, server_reflection_client::ServerReflectionClient,
    server_reflection_request::MessageRequest, server_reflection_response::MessageResponse,
};

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

#[tokio::test]
async fn grpc_health_check_accepts_large_metadata() {
    let config = single_source_config("demo", 1.0, None);
    let harness = TestHarness::new(&config)
        .await
        .expect("Vector should start");
    let url = format!("http://127.0.0.1:{}", harness.api_port());
    let channel = tonic::transport::Endpoint::from_shared(url)
        .expect("API URL should be valid")
        .connect()
        .await
        .expect("health client should connect");
    let mut client = tonic_health::pb::health_client::HealthClient::new(channel);
    let mut request = tonic::Request::new(tonic_health::pb::HealthCheckRequest {
        service: String::new(),
    });
    // The native HTTP/2 server default is only 16 KiB; the API must retain its
    // existing capacity for larger request metadata.
    request.metadata_mut().insert(
        "x-vector-test-padding",
        "x".repeat(24 * 1024).parse().expect("valid ASCII metadata"),
    );
    let response = client
        .check(request)
        .await
        .expect("large request metadata should be accepted")
        .into_inner();

    assert_eq!(
        response.status,
        tonic_health::pb::health_check_response::ServingStatus::Serving as i32
    );
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

/// Verifies the API keeps serving the `grpc.reflection.v1alpha` protocol used by
/// tools such as grpcurl, and that it advertises the observability, health, and
/// reflection services.
#[tokio::test]
async fn grpc_reflection_v1alpha_lists_services() {
    let config = single_source_config("demo", 1.0, None);
    let harness = TestHarness::new(&config)
        .await
        .expect("Vector should start");

    let url = format!("http://127.0.0.1:{}", harness.api_port());
    let channel = tonic::transport::Endpoint::from_shared(url)
        .expect("API URL should be valid")
        .connect()
        .await
        .expect("reflection client should connect");
    let mut client = ServerReflectionClient::new(channel);
    let request = ServerReflectionRequest {
        host: String::new(),
        message_request: Some(MessageRequest::ListServices(String::new())),
    };
    let mut responses = client
        .server_reflection_info(tokio_stream::once(request))
        .await
        .expect("v1alpha reflection should be served")
        .into_inner();
    let response = responses
        .message()
        .await
        .expect("reflection response should be received")
        .expect("reflection stream should yield a response");

    let Some(MessageResponse::ListServicesResponse(list)) = response.message_response else {
        panic!("expected a list services response");
    };
    let services: Vec<_> = list.service.into_iter().map(|s| s.name).collect();
    for expected in [
        "grpc.health.v1.Health",
        "grpc.reflection.v1alpha.ServerReflection",
        "vector.observability.v1.ObservabilityService",
    ] {
        assert!(
            services.iter().any(|name| name == expected),
            "{expected} should be advertised, got {services:?}"
        );
    }
}
