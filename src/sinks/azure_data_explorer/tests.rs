//! Unit and integration tests for the `azure_data_explorer` sink.

use std::{collections::BTreeMap, convert::Infallible};

use futures::{future::ready, stream};
use http::Response;
use hyper::{Body, Request};

use super::{
    auth::AzureDataExplorerAuth,
    config::{AzureDataExplorerConfig, IngestionMethod},
    encoder::AzureDataExplorerEncoder,
    request_builder::AzureDataExplorerRequestBuilder,
    service::{AzureDataExplorerService, IngestConfig},
    sink::{AdxPartitioner, AzureDataExplorerSink},
};
use crate::{
    http::HttpClient,
    sinks::{
        prelude::*,
        util::{http::http_response_retry_logic, service::GlobalTowerRequestConfigDefaults},
    },
    test_util::{
        components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        http::spawn_blackhole_http_server,
    },
};

// ---------------------------------------------------------------------------
// Mock HTTP handlers
// ---------------------------------------------------------------------------

/// Mock handler for streaming ingest (`POST /v1/rest/ingest/...`) and management.
async fn mock_streaming_response(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path().to_string();
    let method = req.method().clone();

    if path.starts_with("/v1/rest/ingest/") && method == http::Method::POST {
        let auth = req
            .headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());
        assert!(
            auth.is_some_and(|a| a.starts_with("Bearer ")),
            "expected Authorization: Bearer on ingest request"
        );
        let q = req.uri().query().unwrap_or("");
        assert!(
            q.contains("streamFormat=MultiJSON"),
            "expected streamFormat=MultiJSON in {q:?}"
        );
        return Ok(Response::builder()
            .status(200)
            .body(Body::from("{}"))
            .unwrap());
    }

    if path.contains("/v1/rest/mgmt") {
        return Ok(Response::builder()
            .status(200)
            .body(Body::from("{}"))
            .unwrap());
    }

    Ok(Response::builder()
        .status(404)
        .body(Body::from("unexpected path"))
        .unwrap())
}

// ---------------------------------------------------------------------------
// Config parse tests
// ---------------------------------------------------------------------------

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<AzureDataExplorerConfig>();
}

#[test]
fn config_parses_with_static_table() {
    let _config: AzureDataExplorerConfig = toml::from_str(
        r#"
            ingestion_endpoint = "https://mycluster.eastus.kusto.windows.net"
            database = "mydb"
            table = "mytable"

            [auth]
            azure_credential_kind = "managed_identity"
        "#,
    )
    .unwrap();
}

#[test]
fn config_parses_with_table_field() {
    let config: AzureDataExplorerConfig = toml::from_str(
        r#"
            ingestion_endpoint = "https://ingest-mycluster.eastus.kusto.windows.net"
            database = "mydb"
            table_field = "adx_table"
            default_table = "fallback"
            ingestion_method = "queued"

            [auth]
            azure_credential_kind = "managed_identity"
        "#,
    )
    .unwrap();

    assert_eq!(config.table_field.as_deref(), Some("adx_table"));
    assert_eq!(config.default_table.as_deref(), Some("fallback"));
    assert!(matches!(config.ingestion_method, IngestionMethod::Queued));
}

#[test]
fn config_parses_with_mapping() {
    let config: AzureDataExplorerConfig = toml::from_str(
        r#"
            ingestion_endpoint = "https://ingest-mycluster.eastus.kusto.windows.net"
            database = "mydb"
            table = "mytable"
            mapping_reference = "my_mapping"

            [auth]
            azure_credential_kind = "managed_identity"
        "#,
    )
    .unwrap();

    assert_eq!(config.mapping_reference.as_deref(), Some("my_mapping"));
}

#[test]
fn config_parses_with_client_secret() {
    let config: AzureDataExplorerConfig = toml::from_str(
        r#"
            ingestion_endpoint = "https://mycluster.eastus.kusto.windows.net"
            database = "mydb"
            table = "mytable"

            [auth]
            azure_credential_kind = "client_secret_credential"
            azure_tenant_id = "my-tenant"
            azure_client_id = "my-client"
            azure_client_secret = "my-secret"
        "#,
    )
    .unwrap();

    assert_eq!(config.database, "mydb");
}

#[test]
fn config_streaming_is_default() {
    let config: AzureDataExplorerConfig = toml::from_str(
        r#"
            ingestion_endpoint = "https://mycluster.eastus.kusto.windows.net"
            database = "mydb"
            table = "mytable"

            [auth]
            azure_credential_kind = "managed_identity"
        "#,
    )
    .unwrap();

    assert!(matches!(config.ingestion_method, IngestionMethod::Streaming));
}

// ---------------------------------------------------------------------------
// Encoder tests
// ---------------------------------------------------------------------------

#[test]
fn encoder_produces_jsonl_without_mutation() {
    use crate::sinks::util::encoding::Encoder as SinkEncoder;

    let encoder = AzureDataExplorerEncoder {
        transformer: Transformer::default(),
    };

    let mut event1 = LogEvent::default();
    event1.insert("message", "hello world");
    event1.insert("host", "server01");

    let mut event2 = LogEvent::default();
    event2.insert("message", "second event");
    event2.insert("count", 42);

    let events = vec![Event::Log(event1), Event::Log(event2)];

    let mut buf = Vec::new();
    let (written, _byte_size) = encoder.encode_input(events, &mut buf).unwrap();
    assert!(written > 0);

    let output = String::from_utf8(buf).unwrap();
    let lines: Vec<&str> = output.split('\n').collect();
    assert_eq!(lines.len(), 2, "Expected 2 JSONL lines, got: {lines:?}");

    let obj1: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(lines[0]).expect("line 0 is valid JSON");
    assert_eq!(
        obj1.get("message").and_then(|v| v.as_str()),
        Some("hello world")
    );
    assert_eq!(obj1.get("host").and_then(|v| v.as_str()), Some("server01"));

    let obj2: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(lines[1]).expect("line 1 is valid JSON");
    assert_eq!(
        obj2.get("message").and_then(|v| v.as_str()),
        Some("second event")
    );
    assert_eq!(obj2.get("count").and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn encoder_single_event_no_trailing_newline() {
    use crate::sinks::util::encoding::Encoder as SinkEncoder;

    let encoder = AzureDataExplorerEncoder {
        transformer: Transformer::default(),
    };

    let mut event = LogEvent::default();
    event.insert("key", "value");

    let events = vec![Event::Log(event)];
    let mut buf = Vec::new();
    encoder.encode_input(events, &mut buf).unwrap();

    let output = String::from_utf8(buf).unwrap();
    assert!(
        !output.ends_with('\n'),
        "single event should have no trailing newline"
    );
    let _: serde_json::Value = serde_json::from_str(&output).expect("output is valid JSON");
}

#[test]
fn encoder_with_transformer_field_filtering() {
    use crate::sinks::util::encoding::Encoder as SinkEncoder;

    let transformer: Transformer = toml::from_str(r#"only_fields = ["message"]"#).unwrap();
    let encoder = AzureDataExplorerEncoder { transformer };

    let mut event = LogEvent::default();
    event.insert("message", "keep me");
    event.insert("host", "drop me");
    event.insert("extra", "also drop");

    let events = vec![Event::Log(event)];
    let mut buf = Vec::new();
    encoder.encode_input(events, &mut buf).unwrap();

    let output = String::from_utf8(buf).unwrap();
    let obj: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(&output).expect("valid JSON");

    assert_eq!(obj.get("message").and_then(|v| v.as_str()), Some("keep me"));
    assert!(obj.get("host").is_none(), "filtered field 'host' should be absent");
    assert!(obj.get("extra").is_none(), "filtered field 'extra' should be absent");
}

// ---------------------------------------------------------------------------
// Table routing (partitioner) tests
// ---------------------------------------------------------------------------

fn make_log_event(fields: &[(&str, &str)]) -> Event {
    let mut log = LogEvent::default();
    for (k, v) in fields {
        log.insert(*k, *v);
    }
    Event::Log(log)
}

fn make_partitioner(
    table_field: Option<&str>,
    table: Option<&str>,
    default_table: Option<&str>,
) -> AdxPartitioner {
    AdxPartitioner {
        table_field: table_field.map(String::from),
        table: table.map(|s| s.try_into().expect("valid template")),
        default_table: default_table.map(String::from),
    }
}

#[test]
fn partitioner_table_field_takes_precedence_over_template() {
    let p = make_partitioner(Some("adx_table"), Some("static_table"), Some("default"));
    let event = make_log_event(&[("adx_table", "from_field")]);
    assert_eq!(
        p.partition(&event),
        Some("from_field".to_string()),
        "table_field should win over template"
    );
}

#[test]
fn partitioner_static_template_used_when_no_field() {
    let p = make_partitioner(None, Some("my_table"), Some("default"));
    let event = make_log_event(&[("msg", "hello")]);
    assert_eq!(p.partition(&event), Some("my_table".to_string()));
}

#[test]
fn partitioner_dynamic_template_resolves_from_event() {
    let p = make_partitioner(None, Some("{{ env }}_logs"), None);
    let event = make_log_event(&[("env", "production")]);
    assert_eq!(
        p.partition(&event),
        Some("production_logs".to_string())
    );
}

#[test]
fn partitioner_falls_back_to_default_on_missing_template_field() {
    let p = make_partitioner(None, Some("{{ missing }}_logs"), Some("fallback_table"));
    let event = make_log_event(&[("msg", "hello")]);
    assert_eq!(p.partition(&event), Some("fallback_table".to_string()));
}

#[test]
fn partitioner_drops_event_when_no_table_resolves() {
    let p = make_partitioner(Some("adx_table"), None, None);
    let event = make_log_event(&[("msg", "hello")]); // "adx_table" field absent
    assert_eq!(p.partition(&event), None, "event should be dropped");
}

#[test]
fn partitioner_default_table_used_when_table_field_absent() {
    let p = make_partitioner(Some("adx_table"), None, Some("default_logs"));
    let event = make_log_event(&[("msg", "hello")]); // "adx_table" absent
    // table_field not found -> no template -> default_table
    assert_eq!(p.partition(&event), Some("default_logs".to_string()));
}

// ---------------------------------------------------------------------------
// Component compliance test (streaming, with mock HTTP server)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn component_spec_compliance_streaming() {
    let mock_endpoint = spawn_blackhole_http_server(mock_streaming_response).await;
    let mock_url = mock_endpoint.to_string();

    let client = HttpClient::new(None, &Default::default()).unwrap();
    let auth = AzureDataExplorerAuth::mock("mock-token-for-testing");
    let compression = Compression::gzip_default();

    let ingest_config = IngestConfig {
        ingestion_endpoint: mock_url,
        database: "testdb".to_string(),
        mapping_reference: None,
        compression,
    };

    let request_builder = AzureDataExplorerRequestBuilder {
        encoder: AzureDataExplorerEncoder {
            transformer: Transformer::default(),
        },
        compression,
    };

    let service = AzureDataExplorerService::new_streaming(client, auth, ingest_config);

    let request_limits =
        TowerRequestConfig::<GlobalTowerRequestConfigDefaults>::default().into_settings();
    let service = ServiceBuilder::new()
        .settings(request_limits, http_response_retry_logic())
        .service(service);

    let batch_settings = BatchConfig::<super::config::StreamingBatchSettings>::default()
        .validate()
        .unwrap()
        .into_batcher_settings()
        .unwrap();

    let partitioner = AdxPartitioner {
        table_field: None,
        table: Some("testtable".try_into().unwrap()),
        default_table: None,
    };

    let sink = AzureDataExplorerSink::new(service, batch_settings, request_builder, partitioner);
    let sink = VectorSink::from_event_streamsink(sink);

    let event = Event::Log(LogEvent::from("simple message"));
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;
}
