//! Service implementation for the `azure_data_explorer` sink.
//!
//! Provides two ingestion modes, selected at construction time:
//!
//! ## Streaming ingestion (default)
//! `POST /v1/rest/ingest/{database}/{table}?streamFormat=MultiJSON`
//! Requires the plain cluster URL (no `ingest-` prefix).
//! See: <https://learn.microsoft.com/en-us/azure/data-explorer/kusto/api/rest/streaming-ingest>
//!
//! ## Queued ingestion
//! 1. Get/refresh ingestion resources (blob + queue SAS URIs via `.get ingestion resources`)
//! 2. PUT payload as a blob to Azure Blob Storage (SAS-authenticated)
//! 3. POST an ingestion notification to Azure Queue Storage (SAS-authenticated)
//! Requires the `ingest-` prefixed endpoint URL.
//!
//! The table name is resolved per-batch by `AdxPartitioner` and carried in
//! the `HttpRequest<String>` context field.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::Engine as _;
use bytes::Bytes;
use futures::future::BoxFuture;
use http::Request;
use tower::Service;
use url::Url;
use uuid::Uuid;

use super::{auth::AzureDataExplorerAuth, resources::ResourceManager};
use crate::{
    http::HttpClient,
    internal_events::{AdxEventsIngested, EndpointBytesSent},
    sinks::{
        prelude::*,
        util::{
            buffer::compression::Compression,
            http::{HttpRequest, HttpResponse},
            uri::protocol_endpoint,
        },
    },
};

// ---------------------------------------------------------------------------
// Shared config
// ---------------------------------------------------------------------------

/// Configuration shared by both ingestion modes.
#[derive(Clone, Debug)]
pub(super) struct IngestConfig {
    pub ingestion_endpoint: String,
    pub database: String,
    pub mapping_reference: Option<String>,
    pub compression: Compression,
}

// ---------------------------------------------------------------------------
// Ingestion mode enum
// ---------------------------------------------------------------------------

/// Selects between streaming and queued ingestion at runtime.
pub(super) enum IngestMode {
    Streaming,
    Queued {
        resource_manager: ResourceManager,
        /// Round-robin blob endpoint index (shared across clones via `Arc`).
        blob_index: Arc<AtomicUsize>,
        /// Round-robin queue endpoint index (shared across clones via `Arc`).
        queue_index: Arc<AtomicUsize>,
    },
}

impl Clone for IngestMode {
    fn clone(&self) -> Self {
        match self {
            Self::Streaming => Self::Streaming,
            Self::Queued {
                resource_manager,
                blob_index,
                queue_index,
            } => Self::Queued {
                resource_manager: resource_manager.clone(),
                blob_index: Arc::clone(blob_index),
                queue_index: Arc::clone(queue_index),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// A Tower `Service` that performs ingestion to Azure Data Explorer.
///
/// The target table for each batch is read from `HttpRequest::context()` (a `String`
/// set by `AzureDataExplorerRequestBuilder`).
pub(super) struct AzureDataExplorerService {
    http_client: HttpClient,
    auth: AzureDataExplorerAuth,
    config: Arc<IngestConfig>,
    mode: IngestMode,
}

impl AzureDataExplorerService {
    pub(super) fn new_streaming(
        http_client: HttpClient,
        auth: AzureDataExplorerAuth,
        config: IngestConfig,
    ) -> Self {
        Self {
            http_client,
            auth,
            config: Arc::new(config),
            mode: IngestMode::Streaming,
        }
    }

    pub(super) fn new_queued(
        http_client: HttpClient,
        auth: AzureDataExplorerAuth,
        config: IngestConfig,
        resource_manager: ResourceManager,
    ) -> Self {
        Self {
            http_client,
            auth,
            config: Arc::new(config),
            mode: IngestMode::Queued {
                resource_manager,
                blob_index: Arc::new(AtomicUsize::new(0)),
                queue_index: Arc::new(AtomicUsize::new(0)),
            },
        }
    }
}

impl Service<HttpRequest<String>> for AzureDataExplorerService {
    type Response = HttpResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: HttpRequest<String>) -> Self::Future {
        let http_client = self.http_client.clone();
        let auth = self.auth.clone();
        let config = Arc::clone(&self.config);
        let mode = self.mode.clone();

        // The table name is stored in the additional_metadata by the request builder.
        let table = request.get_additional_metadata().clone();

        let metadata = std::mem::take(request.metadata_mut());
        let raw_byte_size = metadata.request_encoded_size();
        let event_count = metadata.event_count();
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();
        let payload = request.take_payload();

        Box::pin(async move {
            match mode {
                IngestMode::Streaming => {
                    ingest_streaming(
                        &http_client,
                        &auth,
                        &config,
                        &table,
                        payload,
                        events_byte_size,
                        raw_byte_size,
                        event_count,
                    )
                    .await
                }
                IngestMode::Queued {
                    resource_manager,
                    blob_index,
                    queue_index,
                } => {
                    ingest_queued(
                        &http_client,
                        &config,
                        &table,
                        payload,
                        events_byte_size,
                        raw_byte_size,
                        event_count,
                        &resource_manager,
                        &blob_index,
                        &queue_index,
                    )
                    .await
                }
            }
        })
    }
}

impl Clone for AzureDataExplorerService {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            auth: self.auth.clone(),
            config: Arc::clone(&self.config),
            mode: self.mode.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Streaming ingestion
// ---------------------------------------------------------------------------

async fn ingest_streaming(
    http_client: &HttpClient,
    auth: &AzureDataExplorerAuth,
    config: &IngestConfig,
    table: &str,
    payload: Bytes,
    events_byte_size: GroupedCountByteSize,
    raw_byte_size: usize,
    event_count: usize,
) -> crate::Result<HttpResponse> {
    let ingest_uri = build_streaming_ingest_url(config, table)?;
    let token = auth.get_token().await?;

    debug!(
        message = "Sending streaming ingest request to Azure Data Explorer.",
        uri = %ingest_uri,
        table = %table,
        payload_bytes = payload.len(),
    );

    let (protocol, endpoint) = protocol_endpoint(
        ingest_uri
            .as_str()
            .parse()
            .unwrap_or_else(|_| http::Uri::from_static("https://unknown")),
    );

    let mut req_builder = Request::post(ingest_uri.as_str())
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .header("Connection", "Keep-Alive")
        .header("x-ms-app", "Kusto.Vector")
        .header("x-ms-user", "Kusto.Vector")
        .header(
            "x-ms-client-request-id",
            format!("Vector.Ingest;{}", Uuid::new_v4()),
        );

    if config.compression.content_encoding().is_some() {
        req_builder = req_builder.header("Content-Encoding", "gzip");
    }

    let http_request = req_builder.body(hyper::Body::from(payload))?;
    let response = http_client.send(http_request).await?;
    let status = response.status();
    let body = http_body::Body::collect(response.into_body())
        .await?
        .to_bytes();

    if status.is_success() {
        debug!(message = "Streaming ingest completed successfully.", status = %status, table = %table);
        emit!(EndpointBytesSent {
            byte_size: raw_byte_size,
            protocol: &protocol,
            endpoint: &endpoint,
        });
        emit!(AdxEventsIngested {
            database: &config.database,
            table,
            event_count,
            byte_size: raw_byte_size,
        });
        let synthetic = http::Response::builder()
            .status(http::StatusCode::OK)
            .body(body)
            .unwrap();
        return Ok(HttpResponse {
            http_response: synthetic,
            events_byte_size,
            raw_byte_size,
        });
    }

    let body_str = String::from_utf8_lossy(&body);
    let err_msg = format!(
        "Azure Data Explorer streaming ingest failed: HTTP {} - {}",
        status,
        &body_str[..body_str.len().min(500)]
    );
    error!(message = %err_msg);

    let synthetic = http::Response::builder()
        .status(status)
        .body(Bytes::from(err_msg))
        .unwrap();
    Ok(HttpResponse {
        http_response: synthetic,
        events_byte_size,
        raw_byte_size,
    })
}

/// Builds `POST {ingestion_endpoint}/v1/rest/ingest/{database}/{table}?streamFormat=MultiJSON...`
fn build_streaming_ingest_url(config: &IngestConfig, table: &str) -> crate::Result<Url> {
    let base = config.ingestion_endpoint.trim_end_matches('/');
    let mut url = Url::parse(base).map_err(|e| format!("Invalid ingestion_endpoint URL: {e}"))?;

    url.path_segments_mut()
        .map_err(|_| {
            "ingestion_endpoint must be a hierarchical HTTP(S) URL (e.g. cannot-be-a-base URLs are not supported)"
        })?
        .push("v1")
        .push("rest")
        .push("ingest")
        .push(config.database.as_str())
        .push(table);

    {
        let mut q = url.query_pairs_mut();
        q.append_pair("streamFormat", "MultiJSON");
        if let Some(m) = config.mapping_reference.as_deref() {
            if !m.is_empty() {
                q.append_pair("mappingName", m);
            }
        }
    }

    Ok(url)
}

// ---------------------------------------------------------------------------
// Queued ingestion
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn ingest_queued(
    http_client: &HttpClient,
    config: &IngestConfig,
    table: &str,
    payload: Bytes,
    events_byte_size: GroupedCountByteSize,
    raw_byte_size: usize,
    event_count: usize,
    resource_manager: &ResourceManager,
    blob_index: &AtomicUsize,
    queue_index: &AtomicUsize,
) -> crate::Result<HttpResponse> {
    // 1. Get/refresh ingestion resources
    let resources = resource_manager.get_resources().await?;

    // 2. Select blob endpoint (round-robin)
    let blob_idx = blob_index.fetch_add(1, Ordering::Relaxed) % resources.blob_endpoints.len();
    let blob_ep = &resources.blob_endpoints[blob_idx];

    // 3. Generate unique blob ID
    let epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let blob_id = format!(
        "vector__{}__{}__{}__{epoch_ms}",
        config.database,
        table,
        Uuid::new_v4(),
    );

    let extension = if config.compression.content_encoding().is_some() {
        ".multijson.gz"
    } else {
        ".multijson"
    };

    // 4. Upload payload to blob storage
    let blob_uri = format!(
        "{}/{blob_id}{extension}?{}",
        blob_ep.base_url, blob_ep.sas_token
    );

    debug!(
        message = "Uploading payload to blob storage.",
        blob_base = %blob_ep.base_url,
        blob_id = %blob_id,
        table = %table,
        payload_bytes = payload.len(),
    );

    let blob_request = Request::put(&blob_uri)
        .header("Content-Type", "application/json")
        .header("x-ms-blob-type", "BlockBlob")
        .header("x-ms-version", "2019-12-12")
        .header("x-ms-app", "Kusto.Vector")
        .header("x-ms-user", "Kusto.Vector")
        .body(hyper::Body::from(payload.clone()))?;

    let blob_response = http_client.send(blob_request).await?;
    let blob_status = blob_response.status();

    if blob_status != http::StatusCode::CREATED {
        let body = http_body::Body::collect(blob_response.into_body())
            .await?
            .to_bytes();
        let body_str = String::from_utf8_lossy(&body);
        let err_msg = format!(
            "Blob upload failed: HTTP {} - {}",
            blob_status,
            &body_str[..body_str.len().min(500)]
        );
        error!(message = %err_msg);
        let synthetic = http::Response::builder()
            .status(blob_status)
            .body(Bytes::from(err_msg))
            .unwrap();
        return Ok(HttpResponse {
            http_response: synthetic,
            events_byte_size,
            raw_byte_size,
        });
    }

    // 5. Build full blob URI for the ingestion message
    let full_blob_uri = format!(
        "{}/{blob_id}{extension}?{}",
        blob_ep.base_url, blob_ep.sas_token
    );

    // 6. Create ingestion message
    let ingestion_message = create_ingestion_message(
        &config.database,
        table,
        config.mapping_reference.as_deref(),
        &full_blob_uri,
        payload.len(),
        &resources.identity_token,
    );

    // 7. Base64-encode and wrap in Azure Queue XML
    let message_b64 = base64::engine::general_purpose::STANDARD.encode(&ingestion_message);
    let queue_payload = format!(
        "<QueueMessage><MessageText>{message_b64}</MessageText></QueueMessage>"
    );

    // 8. Enqueue ingestion notification (round-robin)
    let queue_idx = queue_index.fetch_add(1, Ordering::Relaxed) % resources.queue_endpoints.len();
    let queue_ep = &resources.queue_endpoints[queue_idx];
    let queue_uri = format!("{}/messages?{}", queue_ep.base_url, queue_ep.sas_token);

    debug!(
        message = "Enqueueing ingestion notification.",
        queue_base = %queue_ep.base_url,
        table = %table,
    );

    let queue_request = Request::post(&queue_uri)
        .header("Content-Type", "application/atom+xml")
        .header("x-ms-version", "2019-12-12")
        .header("x-ms-app", "Kusto.Vector")
        .header("x-ms-user", "Kusto.Vector")
        .body(hyper::Body::from(queue_payload))?;

    let queue_response = http_client.send(queue_request).await?;
    let queue_status = queue_response.status();

    if queue_status != http::StatusCode::CREATED {
        let body = http_body::Body::collect(queue_response.into_body())
            .await?
            .to_bytes();
        let body_str = String::from_utf8_lossy(&body);
        let err_msg = format!(
            "Queue notification failed: HTTP {} - {}",
            queue_status,
            &body_str[..body_str.len().min(500)]
        );
        error!(message = %err_msg);
        let synthetic = http::Response::builder()
            .status(queue_status)
            .body(Bytes::from(err_msg))
            .unwrap();
        return Ok(HttpResponse {
            http_response: synthetic,
            events_byte_size,
            raw_byte_size,
        });
    }

    debug!(message = "Queued ingestion completed successfully.", blob_id = %blob_id, table = %table);

    // Emit bytes-sent metric (use the queue endpoint as the representative endpoint)
    let (protocol, endpoint) = protocol_endpoint(
        queue_ep
            .base_url
            .parse()
            .unwrap_or_else(|_| http::Uri::from_static("https://unknown")),
    );
    emit!(EndpointBytesSent {
        byte_size: raw_byte_size,
        protocol: &protocol,
        endpoint: &endpoint,
    });
    emit!(AdxEventsIngested {
        database: &config.database,
        table,
        event_count,
        byte_size: raw_byte_size,
    });

    let synthetic = http::Response::builder()
        .status(http::StatusCode::OK)
        .body(Bytes::from("queued"))
        .unwrap();
    Ok(HttpResponse {
        http_response: synthetic,
        events_byte_size,
        raw_byte_size,
    })
}

/// Creates the JSON ingestion message matching the Fluent Bit / ADX format.
fn create_ingestion_message(
    database: &str,
    table: &str,
    mapping_reference: Option<&str>,
    blob_uri: &str,
    raw_data_size: usize,
    identity_token: &str,
) -> String {
    let uuid = Uuid::new_v4();
    let mapping = mapping_reference.unwrap_or("");

    format!(
        r#"{{"Id":"{uuid}","BlobPath":"{blob_uri}","RawDataSize":{raw_data_size},"DatabaseName":"{database}","TableName":"{table}","ClientVersionForTracing":"Kusto.Vector:0.1.0","ApplicationForTracing":"Kusto.Vector","AdditionalProperties":{{"format":"multijson","authorizationContext":"{identity_token}","jsonMappingReference":"{mapping}"}}}}"#,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_ingest_url_basic() {
        let config = IngestConfig {
            ingestion_endpoint: "https://mycluster.eastus.kusto.windows.net".to_string(),
            database: "MyDb".to_string(),
            mapping_reference: None,
            compression: Compression::None,
        };
        let u = build_streaming_ingest_url(&config, "MyTable").unwrap();
        assert_eq!(u.path(), "/v1/rest/ingest/MyDb/MyTable");
        let q: std::collections::HashMap<String, String> = u.query_pairs().into_owned().collect();
        assert_eq!(q.get("streamFormat").map(String::as_str), Some("MultiJSON"));
        assert!(!q.contains_key("mappingName"));
    }

    #[test]
    fn streaming_ingest_url_with_mapping() {
        let config = IngestConfig {
            ingestion_endpoint: "https://ingest.example.com/".to_string(),
            database: "db".to_string(),
            mapping_reference: Some("my_map".to_string()),
            compression: Compression::None,
        };
        let u = build_streaming_ingest_url(&config, "tbl").unwrap();
        assert!(u.as_str().contains("mappingName=my_map"));
        assert!(u.as_str().contains("streamFormat=MultiJSON"));
    }

    #[test]
    fn streaming_ingest_url_uses_per_batch_table() {
        let config = IngestConfig {
            ingestion_endpoint: "https://mycluster.eastus.kusto.windows.net".to_string(),
            database: "MyDb".to_string(),
            mapping_reference: None,
            compression: Compression::None,
        };
        // The table name always comes from the per-batch request context.
        let u = build_streaming_ingest_url(&config, "OverrideTable").unwrap();
        assert_eq!(u.path(), "/v1/rest/ingest/MyDb/OverrideTable");
    }

    #[test]
    fn ingestion_message_format() {
        let msg = create_ingestion_message(
            "testdb",
            "testtable",
            Some("my_mapping"),
            "https://blob.core.windows.net/c/blob.multijson?sas",
            1234,
            "identity_tok",
        );

        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert!(parsed.get("Id").is_some());
        assert_eq!(
            parsed["BlobPath"].as_str().unwrap(),
            "https://blob.core.windows.net/c/blob.multijson?sas"
        );
        assert_eq!(parsed["RawDataSize"].as_u64().unwrap(), 1234);
        assert_eq!(parsed["DatabaseName"].as_str().unwrap(), "testdb");
        assert_eq!(parsed["TableName"].as_str().unwrap(), "testtable");
        assert_eq!(
            parsed["AdditionalProperties"]["format"].as_str().unwrap(),
            "multijson"
        );
        assert_eq!(
            parsed["AdditionalProperties"]["authorizationContext"]
                .as_str()
                .unwrap(),
            "identity_tok"
        );
        assert_eq!(
            parsed["AdditionalProperties"]["jsonMappingReference"]
                .as_str()
                .unwrap(),
            "my_mapping"
        );
    }

    #[test]
    fn ingestion_message_no_mapping() {
        let msg = create_ingestion_message(
            "db",
            "tbl",
            None,
            "https://blob/path?sas",
            42,
            "tok",
        );
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(
            parsed["AdditionalProperties"]["jsonMappingReference"]
                .as_str()
                .unwrap(),
            ""
        );
    }
}
