use bytes::Bytes;
use futures::stream;
use http_0::Response;
use std::time::Duration;
use tokio::time::timeout;
use vector_lib::config::log_schema;

use azure_core::credentials::{AccessToken, TokenCredential};
use azure_core::time::OffsetDateTime;

use crate::sinks::azure_common::config::{AzureAuthentication, SpecificAzureCredential};

use super::config::AzureLogsIngestionConfig;

use crate::{
    config::ValidatedSink,
    event::LogEvent,
    sinks::{prelude::*, util::HttpEndpoint},
    test_util::{
        components::{SINK_TAGS, run_and_assert_sink_compliance},
        http::spawn_blackhole_http_server,
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<AzureLogsIngestionConfig>();
}

#[test]
fn validate_accepts_valid_config() {
    let config: AzureLogsIngestionConfig = serde_yaml::from_str(indoc::indoc! {r#"
            endpoint: "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
            dcr_immutable_id: dcr-00000000000000000000000000000000
            stream_name: Custom-UnitTest
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
    .unwrap();

    config.validate().expect("validation should succeed");
}

#[test]
fn validate_rejects_invalid_path_segments() {
    // A space in either path segment makes the request path unparseable, so
    // validation must fail rather than panicking at build time.
    let config: AzureLogsIngestionConfig = serde_yaml::from_str(indoc::indoc! {r#"
            endpoint: "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
            dcr_immutable_id: "dcr-00000000000000000000000000000000 with space"
            stream_name: Custom-UnitTest
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
    .unwrap();

    config
        .validate()
        .expect_err("validation should reject a dcr_immutable_id containing a space");

    let config: AzureLogsIngestionConfig = serde_yaml::from_str(indoc::indoc! {r#"
            endpoint: "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
            dcr_immutable_id: dcr-00000000000000000000000000000000
            stream_name: "Custom-Unit Test"
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
    .unwrap();

    config
        .validate()
        .expect_err("validation should reject a stream_name containing a space");
}

#[test]
fn validate_rejects_non_http_endpoint() {
    // The `HttpEndpoint` config field rejects a non-http(s) endpoint at load
    // time, so deserialization fails.
    let result: Result<AzureLogsIngestionConfig, _> = serde_yaml::from_str(indoc::indoc! {r#"
            endpoint: "ftp://example.com"
            dcr_immutable_id: dcr-00000000000000000000000000000000
            stream_name: Custom-UnitTest
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#});
    assert!(
        result.is_err(),
        "config load should reject a non-http endpoint"
    );
}

#[tokio::test]
async fn basic_config_error_with_no_auth() {
    let config: Result<AzureLogsIngestionConfig, _> =
        serde_yaml::from_str::<AzureLogsIngestionConfig>(indoc::indoc! {r#"
            endpoint: "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
            dcr_immutable_id: dcr-00000000000000000000000000000000
            stream_name: Custom-UnitTest
        "#});

    match config {
        Ok(_) => panic!("Config parsing should have failed due to missing auth config"),
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("missing field `auth`"),
                "Config parsing did not complain about missing auth field: {err_str}"
            );
        }
    }
}

#[test]
fn basic_config_with_client_credentials() {
    let config: AzureLogsIngestionConfig =
        serde_yaml::from_str::<AzureLogsIngestionConfig>(indoc::indoc! {r#"
            endpoint: "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
            dcr_immutable_id: dcr-00000000000000000000000000000000
            stream_name: Custom-UnitTest
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
        .expect("Config parsing failed");

    assert_eq!(
        config.endpoint.to_string(),
        "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com/"
    );
    assert_eq!(
        config.dcr_immutable_id,
        "dcr-00000000000000000000000000000000"
    );
    assert_eq!(config.stream_name, "Custom-UnitTest");
    assert_eq!(config.token_scope, "https://monitor.azure.com/.default");
    assert_eq!(config.timestamp_field, "TimeGenerated");

    match &config.auth {
        AzureAuthentication::Specific(SpecificAzureCredential::ClientSecretCredential {
            azure_tenant_id,
            azure_client_id,
            azure_client_secret,
        }) => {
            assert_eq!(azure_tenant_id, "00000000-0000-0000-0000-000000000000");
            assert_eq!(azure_client_id, "mock-client-id");
            let secret: String = azure_client_secret.inner().into();
            assert_eq!(secret, "mock-client-secret");
        }
        _ => panic!("Expected Specific(ClientSecretCredential) variant"),
    }
}

#[test]
fn basic_config_with_managed_identity() {
    let config: AzureLogsIngestionConfig =
        serde_yaml::from_str::<AzureLogsIngestionConfig>(indoc::indoc! {r#"
            endpoint: "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
            dcr_immutable_id: dcr-00000000000000000000000000000000
            stream_name: Custom-UnitTest
            auth:
              azure_credential_kind: managed_identity
        "#})
        .expect("Config parsing failed");

    assert_eq!(
        config.endpoint.to_string(),
        "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com/"
    );
    assert_eq!(
        config.dcr_immutable_id,
        "dcr-00000000000000000000000000000000"
    );
    assert_eq!(config.stream_name, "Custom-UnitTest");
    assert_eq!(config.token_scope, "https://monitor.azure.com/.default");
    assert_eq!(config.timestamp_field, "TimeGenerated");

    match &config.auth {
        AzureAuthentication::Specific(SpecificAzureCredential::ManagedIdentity { .. }) => {
            // Expected variant
        }
        _ => panic!("Expected Specific(ManagedIdentity) variant"),
    }
}

// TODO test config with ManagedIdentity (will need to mock env vars...)

fn insert_timestamp_kv(log: &mut LogEvent) -> (String, String) {
    let now = chrono::Utc::now();

    let timestamp_value = now.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    log.insert(log_schema().timestamp_key_target_path().unwrap(), now);

    (
        log_schema().timestamp_key().unwrap().to_string(),
        timestamp_value,
    )
}

#[tokio::test]
async fn correct_request() {
    let credential = std::sync::Arc::new(create_mock_credential());

    let config: AzureLogsIngestionConfig = serde_yaml::from_str(indoc::indoc! {r#"
            endpoint: "http://localhost:9001"
            dcr_immutable_id: dcr-00000000000000000000000000000000
            stream_name: Custom-UnitTest
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
    .unwrap();

    let mut log1 = [("message", "hello")].iter().copied().collect::<LogEvent>();
    let (_timestamp_key1, timestamp_value1) = insert_timestamp_kv(&mut log1);

    let mut log2 = [("message", "world")].iter().copied().collect::<LogEvent>();
    let (_timestamp_key2, timestamp_value2) = insert_timestamp_kv(&mut log2);

    let (endpoint_tx, mut endpoint_rx) = tokio::sync::mpsc::channel(1);
    let mock_endpoint = spawn_blackhole_http_server(move |request| {
        let endpoint_tx = endpoint_tx.clone();
        async move {
            endpoint_tx.send(request).await.unwrap();
            Ok(Response::builder()
                .status(204)
                .body(hyper::Body::empty())
                .unwrap())
        }
    })
    .await;

    let context = SinkContext::default();

    let validated = config.validate().unwrap();
    let (sink, healthcheck) = config
        .build_inner(
            &context,
            &validated,
            HttpEndpoint::parse(&mock_endpoint.to_string()).unwrap(),
            credential,
            config.token_scope.clone(),
            config.timestamp_field.clone(),
        )
        .unwrap();

    run_and_assert_sink_compliance(sink, stream::iter(vec![log1, log2]), &SINK_TAGS).await;

    let request = timeout(Duration::from_millis(500), endpoint_rx.recv())
        .await
        .unwrap()
        .unwrap();

    let (parts, body) = request.into_parts();
    assert_eq!(&parts.method.to_string(), "POST");

    let body_bytes: Bytes = http_body_0::Body::collect(body)
        .await
        .expect("failed to collect body")
        .to_bytes();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes[..]).unwrap();
    let expected_json = serde_json::json!([
        {
            "TimeGenerated": timestamp_value1,
            "message": "hello"
        },
        {
            "TimeGenerated": timestamp_value2,
            "message": "world"
        }
    ]);
    assert_eq!(body_json, expected_json);

    healthcheck.await.expect("Healthcheck failed");

    let headers = parts.headers;
    let authorization = headers.get("Authorization").unwrap();
    assert_eq!(authorization.to_str().unwrap(), "Bearer mock-access-token");

    assert_eq!(
        &parts.uri.path_and_query().unwrap().to_string(),
        "/dataCollectionRules/dcr-00000000000000000000000000000000/streams/Custom-UnitTest?api-version=2023-01-01"
    );
}

#[tokio::test]
async fn retries_transport_error() {
    use hyper::{
        Body, Server,
        service::{make_service_fn, service_fn},
    };
    use std::{
        convert::Infallible,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    let (_guard, addr) = crate::test_util::addr::next_addr();
    let requests = Arc::new(AtomicUsize::new(0));
    let server_requests = Arc::clone(&requests);
    let server = Server::bind(&addr).serve(make_service_fn(move |_| {
        let requests = Arc::clone(&server_requests);
        async move {
            Ok::<_, Infallible>(service_fn(move |request: http_0::Request<Body>| {
                let requests = Arc::clone(&requests);
                async move {
                    http_body_0::Body::collect(request.into_body())
                        .await
                        .unwrap();
                    if requests.fetch_add(1, Ordering::Relaxed) == 0 {
                        Err(std::io::Error::other(
                            "close the first connection without a response",
                        ))
                    } else {
                        Ok(Response::builder().status(204).body(Body::empty()).unwrap())
                    }
                }
            }))
        }
    }));
    let server = tokio::spawn(server);
    let config: AzureLogsIngestionConfig = serde_yaml::from_str(indoc::indoc! {r#"
        endpoint: "http://localhost:9001"
        dcr_immutable_id: dcr-00000000000000000000000000000000
        stream_name: Custom-UnitTest
        request:
          retry_attempts: 1
        auth:
          azure_credential_kind: client_secret_credential
          azure_tenant_id: "00000000-0000-0000-0000-000000000000"
          azure_client_id: mock-client-id
          azure_client_secret: mock-client-secret
    "#})
    .unwrap();
    let (sink, _) = config
        .build_inner(
            &SinkContext::default(),
            &config.validate().unwrap(),
            HttpEndpoint::parse(&format!("http://{addr}")).unwrap(),
            Arc::new(create_mock_credential()),
            config.token_scope.clone(),
            config.timestamp_field.clone(),
        )
        .unwrap();
    let (batch, receiver) = crate::event::BatchNotifier::new_with_receiver();
    let mut log = LogEvent::from("retry after connection closes");
    insert_timestamp_kv(&mut log);
    let events =
        crate::test_util::map_event_batch_stream(stream::iter([Event::Log(log)]), Some(batch));
    let status = timeout(Duration::from_secs(10), async {
        sink.run(events).await.unwrap();
        receiver.await
    })
    .await;
    server.abort();

    assert_eq!(status.unwrap(), crate::event::BatchStatus::Delivered);
    assert_eq!(requests.load(Ordering::Relaxed), 2);
}

fn create_mock_credential() -> impl TokenCredential {
    #[derive(Debug)]
    struct MockCredential;

    #[async_trait::async_trait]
    impl TokenCredential for MockCredential {
        async fn get_token(
            &self,
            _scopes: &[&str],
            _options: Option<azure_core::credentials::TokenRequestOptions<'_>>,
        ) -> azure_core::Result<AccessToken> {
            Ok(AccessToken::new(
                "mock-access-token".to_string(),
                OffsetDateTime::now_utc() + Duration::from_hours(1),
            ))
        }
    }

    MockCredential
}

#[tokio::test]
async fn mock_healthcheck_with_400_response() {
    let config: AzureLogsIngestionConfig = serde_yaml::from_str(indoc::indoc! {r#"
            endpoint: "http://localhost:9001"
            dcr_immutable_id: dcr-00000000000000000000000000000000
            stream_name: Custom-UnitTest
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
    .unwrap();

    let mut log1 = [("message", "hello")].iter().copied().collect::<LogEvent>();
    let (_timestamp_key1, _timestamp_value1) = insert_timestamp_kv(&mut log1);

    let (endpoint_tx, _endpoint_rx) = tokio::sync::mpsc::channel(1);
    let mock_endpoint = spawn_blackhole_http_server(move |request| {
        let endpoint_tx = endpoint_tx.clone();
        async move {
            endpoint_tx.send(request).await.unwrap();
            let body = serde_json::json!({
                "error": "Mock400ErrorResponse",
            })
            .to_string();

            Ok(Response::builder()
                .status(400)
                .header("Content-Type", "application/json")
                .body(body.into())
                .unwrap())
        }
    })
    .await;

    let context = SinkContext::default();
    let credential = std::sync::Arc::new(create_mock_credential());

    let validated = config.validate().unwrap();
    let (_sink, healthcheck) = config
        .build_inner(
            &context,
            &validated,
            HttpEndpoint::parse(&mock_endpoint.to_string()).unwrap(),
            credential,
            config.token_scope.clone(),
            config.timestamp_field.clone(),
        )
        .unwrap();

    let hc_err = healthcheck.await.unwrap_err();
    let err_str = hc_err.to_string();
    // Both generic 400 "Bad Request", and our mock error message should be present
    assert!(
        err_str.contains("Bad Request"),
        "Healthcheck error does not contain 'Bad Request': {err_str}"
    );
    assert!(
        err_str.contains("Mock400ErrorResponse"),
        "Healthcheck error does not contain 'Mock400ErrorResponse': {err_str}"
    );
}

#[tokio::test]
async fn mock_healthcheck_with_403_response() {
    let config: AzureLogsIngestionConfig = serde_yaml::from_str(indoc::indoc! {r#"
            endpoint: "http://localhost:9001"
            dcr_immutable_id: dcr-00000000000000000000000000000000
            stream_name: Custom-UnitTest
            auth:
              azure_credential_kind: client_secret_credential
              azure_tenant_id: "00000000-0000-0000-0000-000000000000"
              azure_client_id: mock-client-id
              azure_client_secret: mock-client-secret
        "#})
    .unwrap();

    let mut log1 = [("message", "hello")].iter().copied().collect::<LogEvent>();
    let (_timestamp_key1, _timestamp_value1) = insert_timestamp_kv(&mut log1);

    let (endpoint_tx, _endpoint_rx) = tokio::sync::mpsc::channel(1);
    let mock_endpoint = spawn_blackhole_http_server(move |request| {
        let endpoint_tx = endpoint_tx.clone();
        async move {
            endpoint_tx.send(request).await.unwrap();
            let body = serde_json::json!({
                "error": "bla",
            })
            .to_string();

            Ok(Response::builder()
                .status(403)
                .header("Content-Type", "application/json")
                .body(body.into())
                .unwrap())
        }
    })
    .await;

    let context = SinkContext::default();
    let credential = std::sync::Arc::new(create_mock_credential());

    let validated = config.validate().unwrap();
    let (_sink, healthcheck) = config
        .build_inner(
            &context,
            &validated,
            HttpEndpoint::parse(&mock_endpoint.to_string()).unwrap(),
            credential,
            config.token_scope.clone(),
            config.timestamp_field.clone(),
        )
        .unwrap();

    let hc_err = healthcheck.await.unwrap_err();
    let err_str = hc_err.to_string();
    assert!(
        err_str.contains("Forbidden"),
        "Healthcheck error does not contain 'Forbidden': {err_str}"
    );
}
