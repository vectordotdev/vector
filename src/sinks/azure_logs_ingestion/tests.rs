use bytes::Bytes;
use futures::stream;
use http::Response;
use std::time::Duration;
use tokio::time::timeout;
use vector_lib::config::log_schema;

use azure_core::credentials::{AccessToken, TokenCredential};
use azure_core::time::OffsetDateTime;

use super::config::AzureLogsIngestionConfig;

use crate::{
    event::LogEvent,
    sinks::prelude::*,
    test_util::{
        components::{SINK_TAGS, run_and_assert_sink_compliance},
        http::spawn_blackhole_http_server,
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<AzureLogsIngestionConfig>();
}

#[tokio::test]
async fn basic_config_error_with_no_auth() {
    let config: AzureLogsIngestionConfig = toml::from_str::<AzureLogsIngestionConfig>(
        r#"
            endpoint = "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
            dcr_immutable_id = "dcr-00000000000000000000000000000000"
            stream_name = "Custom-UnitTest"
        "#,
    )
    .expect("Config parsing failed");

    assert_eq!(
        config.endpoint,
        "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
    );
    assert_eq!(
        config.dcr_immutable_id,
        "dcr-00000000000000000000000000000000"
    );
    assert_eq!(config.stream_name, "Custom-UnitTest");
    assert_eq!(config.token_scope, "https://monitor.azure.com/.default");
    assert_eq!(config.timestamp_field, "TimeGenerated");

    match &config.auth {
        crate::sinks::azure_logs_ingestion::config::AzureAuthentication::ClientSecretCredential {
            azure_tenant_id,
            azure_client_id,
            azure_client_secret,
        } => {
            assert_eq!(azure_tenant_id, "");
            assert_eq!(azure_client_id, "");
            let secret: String = azure_client_secret.inner().into();
            assert_eq!(secret, "");
        }
        _ => panic!("Expected ClientSecretCredential variant"),
    }

    let cx = SinkContext::default();
    let sink = config.build(cx).await;
    match sink {
        Ok(_) => panic!("Config build should have errored due to missing auth info"),
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("`auth.azure_tenant_id` is blank"),
                "Config build did not complain about azure_tenant_id being blank: {}",
                err_str
            );
        }
    }
}

#[test]
fn basic_config_with_client_credentials() {
    let config: AzureLogsIngestionConfig = toml::from_str::<AzureLogsIngestionConfig>(
        r#"
            endpoint = "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
            dcr_immutable_id = "dcr-00000000000000000000000000000000"
            stream_name = "Custom-UnitTest"

            [auth]
            azure_tenant_id = "00000000-0000-0000-0000-000000000000"
            azure_client_id = "mock-client-id"
            azure_client_secret = "mock-client-secret"
        "#,
    )
    .expect("Config parsing failed");

    assert_eq!(
        config.endpoint,
        "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
    );
    assert_eq!(
        config.dcr_immutable_id,
        "dcr-00000000000000000000000000000000"
    );
    assert_eq!(config.stream_name, "Custom-UnitTest");
    assert_eq!(config.token_scope, "https://monitor.azure.com/.default");
    assert_eq!(config.timestamp_field, "TimeGenerated");

    match &config.auth {
        crate::sinks::azure_logs_ingestion::config::AzureAuthentication::ClientSecretCredential {
            azure_tenant_id,
            azure_client_id,
            azure_client_secret,
        } => {
            assert_eq!(azure_tenant_id, "00000000-0000-0000-0000-000000000000");
            assert_eq!(azure_client_id, "mock-client-id");
            let secret: String = azure_client_secret.inner().into();
            assert_eq!(secret, "mock-client-secret");
        }
        _ => panic!("Expected ClientSecretCredential variant"),
    }
}

#[test]
fn basic_config_with_managed_identity() {
    let config: AzureLogsIngestionConfig = toml::from_str::<AzureLogsIngestionConfig>(
        r#"
            endpoint = "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
            dcr_immutable_id = "dcr-00000000000000000000000000000000"
            stream_name = "Custom-UnitTest"

            [auth]
            azure_credential_kind = "managedidentity"
        "#,
    )
    .expect("Config parsing failed");

    assert_eq!(
        config.endpoint,
        "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
    );
    assert_eq!(
        config.dcr_immutable_id,
        "dcr-00000000000000000000000000000000"
    );
    assert_eq!(config.stream_name, "Custom-UnitTest");
    assert_eq!(config.token_scope, "https://monitor.azure.com/.default");
    assert_eq!(config.timestamp_field, "TimeGenerated");

    match &config.auth {
        crate::sinks::azure_logs_ingestion::config::AzureAuthentication::Specific(
            crate::sinks::azure_logs_ingestion::config::SpecificAzureCredential::ManagedIdentity { .. }
        ) => {
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

    let config: AzureLogsIngestionConfig = toml::from_str(
        r#"
            endpoint = "http://localhost:9001"
            dcr_immutable_id = "dcr-00000000000000000000000000000000"
            stream_name = "Custom-UnitTest"

            [auth]
            azure_tenant_id = "00000000-0000-0000-0000-000000000000"
            azure_client_id = "mock-client-id"
            azure_client_secret = "mock-client-secret"
        "#,
    )
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

    let (sink, healthcheck) = config
        .build_inner(
            context,
            mock_endpoint.into(),
            config.dcr_immutable_id.clone(),
            config.stream_name.clone(),
            credential,
            config.token_scope.clone(),
            config.timestamp_field.clone(),
        )
        .await
        .unwrap();

    run_and_assert_sink_compliance(sink, stream::iter(vec![log1, log2]), &SINK_TAGS).await;

    let request = timeout(Duration::from_millis(500), endpoint_rx.recv())
        .await
        .unwrap()
        .unwrap();

    let (parts, body) = request.into_parts();
    assert_eq!(&parts.method.to_string(), "POST");

    let body_bytes: Bytes = http_body::Body::collect(body)
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

    let _healthcheck_message = healthcheck.await.expect("Healthcheck failed");

    let headers = parts.headers;
    let authorization = headers.get("Authorization").unwrap();
    assert_eq!(authorization.to_str().unwrap(), "Bearer mock-access-token");

    assert_eq!(
        &parts.uri.path_and_query().unwrap().to_string(),
        "/dataCollectionRules/dcr-00000000000000000000000000000000/streams/Custom-UnitTest?api-version=2023-01-01"
    );
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
    let config: AzureLogsIngestionConfig = toml::from_str(
        r#"
            endpoint = "http://localhost:9001"
            dcr_immutable_id = "dcr-00000000000000000000000000000000"
            stream_name = "Custom-UnitTest"

            [auth]
            azure_tenant_id = "00000000-0000-0000-0000-000000000000"
            azure_client_id = "mock-client-id"
            azure_client_secret = "mock-client-secret"
        "#,
    )
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

    let (_sink, healthcheck) = config
        .build_inner(
            context,
            mock_endpoint.into(),
            config.dcr_immutable_id.clone(),
            config.stream_name.clone(),
            credential,
            config.token_scope.clone(),
            config.timestamp_field.clone(),
        )
        .await
        .unwrap();

    let hc_err = healthcheck.await.unwrap_err();
    let err_str = hc_err.to_string();
    // Both generic 400 "Bad Request", and our mock error message should be present
    assert!(
        err_str.contains("Bad Request"),
        "Healthcheck error does not contain 'Bad Request': {}",
        err_str
    );
    assert!(
        err_str.contains("Mock400ErrorResponse"),
        "Healthcheck error does not contain 'Mock400ErrorResponse': {}",
        err_str
    );
}

#[tokio::test]
async fn mock_healthcheck_with_403_response() {
    let config: AzureLogsIngestionConfig = toml::from_str(
        r#"
            endpoint = "http://localhost:9001"
            dcr_immutable_id = "dcr-00000000000000000000000000000000"
            stream_name = "Custom-UnitTest"

            [auth]
            azure_tenant_id = "00000000-0000-0000-0000-000000000000"
            azure_client_id = "mock-client-id"
            azure_client_secret = "mock-client-secret"
        "#,
    )
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

    let (_sink, healthcheck) = config
        .build_inner(
            context,
            mock_endpoint.into(),
            config.dcr_immutable_id.clone(),
            config.stream_name.clone(),
            credential,
            config.token_scope.clone(),
            config.timestamp_field.clone(),
        )
        .await
        .unwrap();

    let hc_err = healthcheck.await.unwrap_err();
    let err_str = hc_err.to_string();
    assert!(
        err_str.contains("Forbidden"),
        "Healthcheck error does not contain 'Forbidden': {}",
        err_str
    );
}
