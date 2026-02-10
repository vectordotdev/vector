//! Service implementation for the `azure_data_explorer` sink.
//!
//! Implements **queued ingestion** matching the Fluent Bit `out_azure_kusto` plugin:
//!
//! 1. Ensure ingestion resources (blob + queue SAS URIs) are loaded and fresh.
//! 2. Upload the JSONL/MultiJSON payload as a blob to Azure Blob Storage (SAS-authenticated PUT).
//! 3. Enqueue an ingestion notification message to Azure Queue Storage (SAS-authenticated POST).
//!
//! The blob and queue endpoints are discovered via the ADX management command
//! `.get ingestion resources` and cached for 1 hour (matching Fluent Bit defaults).

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::Engine as _;
use bytes::Bytes;
use futures::future::BoxFuture;
use http::Request;
use tower::Service;
use uuid::Uuid;

use super::resources::ResourceManager;
use crate::{
    http::HttpClient,
    sinks::{
        prelude::*,
        util::{
            buffer::compression::Compression,
            http::{HttpRequest, HttpResponse},
        },
    },
};

// ---------------------------------------------------------------------------
// Queued ingest configuration (shared across clones)
// ---------------------------------------------------------------------------

/// Configuration for the queued ingest service, shared across all clones.
#[derive(Clone, Debug)]
pub(super) struct QueuedIngestConfig {
    pub database: String,
    pub table: String,
    pub mapping_reference: Option<String>,
    pub compression: Compression,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// A Tower `Service` that performs **queued ingestion** to Azure Data Explorer.
///
/// Each `call()` invocation:
/// 1. Gets/refreshes ingestion resources (blob + queue SAS URIs + identity token)
/// 2. Uploads the payload to a randomly selected blob endpoint
/// 3. Enqueues an ingestion notification to a randomly selected queue endpoint
/// 4. Returns a synthetic `HttpResponse` indicating success or the first failure
pub(super) struct AzureDataExplorerService {
    http_client: HttpClient,
    resource_manager: ResourceManager,
    config: Arc<QueuedIngestConfig>,
    /// Round-robin index for blob endpoint selection.
    blob_index: Arc<AtomicUsize>,
    /// Round-robin index for queue endpoint selection.
    queue_index: Arc<AtomicUsize>,
}

impl AzureDataExplorerService {
    pub(super) fn new(
        http_client: HttpClient,
        resource_manager: ResourceManager,
        config: QueuedIngestConfig,
    ) -> Self {
        Self {
            http_client,
            resource_manager,
            config: Arc::new(config),
            blob_index: Arc::new(AtomicUsize::new(0)),
            queue_index: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Service<HttpRequest<()>> for AzureDataExplorerService {
    type Response = HttpResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: HttpRequest<()>) -> Self::Future {
        let http_client = self.http_client.clone();
        let resource_manager = self.resource_manager.clone();
        let config = Arc::clone(&self.config);
        let blob_index = Arc::clone(&self.blob_index);
        let queue_index = Arc::clone(&self.queue_index);

        let metadata = std::mem::take(request.metadata_mut());
        let raw_byte_size = metadata.request_encoded_size();
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();
        let payload = request.take_payload();

        Box::pin(async move {
            // 1. Get/refresh ingestion resources
            let resources = resource_manager.get_resources().await?;

            // 2. Select blob endpoint (round-robin)
            let blob_idx =
                blob_index.fetch_add(1, Ordering::Relaxed) % resources.blob_endpoints.len();
            let blob_ep = &resources.blob_endpoints[blob_idx];

            // 3. Generate unique blob ID
            let epoch_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let blob_id = format!(
                "vector__{}__{}__{}__{epoch_ms}",
                config.database,
                config.table,
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
                blob_uri = %blob_ep.base_url,
                blob_id = %blob_id,
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

                // Return a synthetic response with the blob's error status code
                // so the retry logic can decide whether to retry.
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

            // 5. Build the full blob URI for the ingestion message
            let full_blob_uri = format!(
                "{}/{blob_id}{extension}?{}",
                blob_ep.base_url, blob_ep.sas_token
            );

            // 6. Create ingestion message (matching Fluent Bit's format)
            let ingestion_message = create_ingestion_message(
                &config,
                &full_blob_uri,
                payload.len(),
                &resources.identity_token,
            );

            // 7. Base64-encode the message and wrap in Azure Queue XML format
            let message_b64 = base64::engine::general_purpose::STANDARD.encode(&ingestion_message);
            let queue_payload = format!(
                "<QueueMessage><MessageText>{message_b64}</MessageText></QueueMessage>"
            );

            // 8. Enqueue ingestion notification (round-robin)
            let queue_idx =
                queue_index.fetch_add(1, Ordering::Relaxed) % resources.queue_endpoints.len();
            let queue_ep = &resources.queue_endpoints[queue_idx];
            let queue_uri = format!("{}/messages?{}", queue_ep.base_url, queue_ep.sas_token);

            debug!(
                message = "Enqueueing ingestion notification.",
                queue_uri = %queue_ep.base_url,
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

            debug!(message = "Queued ingestion completed successfully.", blob_id = %blob_id);

            // 9. Return synthetic 200 OK
            let synthetic = http::Response::builder()
                .status(http::StatusCode::OK)
                .body(Bytes::from("queued"))
                .unwrap();

            Ok(HttpResponse {
                http_response: synthetic,
                events_byte_size,
                raw_byte_size,
            })
        })
    }
}

impl Clone for AzureDataExplorerService {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            resource_manager: self.resource_manager.clone(),
            config: Arc::clone(&self.config),
            blob_index: Arc::clone(&self.blob_index),
            queue_index: Arc::clone(&self.queue_index),
        }
    }
}

// ---------------------------------------------------------------------------
// Ingestion message helpers
// ---------------------------------------------------------------------------

/// Creates the JSON ingestion message matching Fluent Bit's format:
///
/// ```json
/// {
///   "Id": "<uuid>",
///   "BlobPath": "<full_blob_url_with_sas>",
///   "RawDataSize": <bytes>,
///   "DatabaseName": "<db>",
///   "TableName": "<table>",
///   "AdditionalProperties": {
///     "format": "multijson",
///     "authorizationContext": "<identity_token>",
///     "jsonMappingReference": "<mapping>"
///   }
/// }
/// ```
fn create_ingestion_message(
    config: &QueuedIngestConfig,
    blob_uri: &str,
    raw_data_size: usize,
    identity_token: &str,
) -> String {
    let uuid = Uuid::new_v4();
    let mapping = config.mapping_reference.as_deref().unwrap_or("");

    // Use format! to build compact JSON matching Fluent Bit's output.
    format!(
        r#"{{"Id":"{uuid}","BlobPath":"{blob_uri}","RawDataSize":{raw_data_size},"DatabaseName":"{}","TableName":"{}","ClientVersionForTracing":"Kusto.Vector:0.1.0","ApplicationForTracing":"Kusto.Vector","AdditionalProperties":{{"format":"multijson","authorizationContext":"{identity_token}","jsonMappingReference":"{mapping}"}}}}"#,
        config.database, config.table,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingestion_message_format() {
        let config = QueuedIngestConfig {
            database: "testdb".to_string(),
            table: "testtable".to_string(),
            mapping_reference: Some("my_mapping".to_string()),
            compression: Compression::None,
        };

        let msg = create_ingestion_message(
            &config,
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
        let config = QueuedIngestConfig {
            database: "db".to_string(),
            table: "tbl".to_string(),
            mapping_reference: None,
            compression: Compression::None,
        };

        let msg = create_ingestion_message(&config, "https://blob/path?sas", 42, "tok");
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(
            parsed["AdditionalProperties"]["jsonMappingReference"]
                .as_str()
                .unwrap(),
            ""
        );
    }
}
