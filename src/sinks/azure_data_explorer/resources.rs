//! Ingestion resource discovery and caching for Azure Data Explorer queued ingestion.
//!
//! Implements the `.get ingestion resources` and `.get kusto identity token`
//! management commands to discover blob storage and queue endpoints.
//! Matches the Fluent Bit `azure_kusto_conf.c` / `azure_kusto_ingest.c` flow.

use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use http::Request;
use serde_json::Value;
use tokio::sync::RwLock;

use super::auth::AzureDataExplorerAuth;
use crate::http::HttpClient;

/// Default refresh interval for ingestion resources (1 hour, matching Fluent Bit).
const RESOURCES_REFRESH_INTERVAL_SECS: u64 = 3600;

/// ADX management endpoint path.
const MGMT_URI_PATH: &str = "/v1/rest/mgmt";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single storage endpoint (blob or queue) with its SAS token.
#[derive(Clone, Debug)]
pub(super) struct StorageEndpoint {
    /// Full base URL without query string
    /// e.g. `https://host.blob.core.windows.net/container`
    pub base_url: String,
    /// SAS query string (without leading `?`)
    pub sas_token: String,
}

/// Cached ingestion resources returned by the ADX management endpoint.
#[derive(Clone, Debug)]
pub(super) struct IngestionResources {
    pub blob_endpoints: Vec<StorageEndpoint>,
    pub queue_endpoints: Vec<StorageEndpoint>,
    pub identity_token: String,
    loaded_at: Instant,
}

impl IngestionResources {
    fn is_stale(&self) -> bool {
        self.loaded_at.elapsed().as_secs() >= RESOURCES_REFRESH_INTERVAL_SECS
    }
}

// ---------------------------------------------------------------------------
// ResourceManager
// ---------------------------------------------------------------------------

/// Manages ingestion resource discovery and caching.
///
/// Periodically refreshes blob/queue SAS URIs and the Kusto identity token
/// by executing management commands against the ADX ingestion endpoint.
#[derive(Clone)]
pub(super) struct ResourceManager {
    auth: AzureDataExplorerAuth,
    http_client: HttpClient,
    ingestion_endpoint: String,
    cached: Arc<RwLock<Option<IngestionResources>>>,
}

impl ResourceManager {
    pub(super) fn new(
        auth: AzureDataExplorerAuth,
        http_client: HttpClient,
        ingestion_endpoint: String,
    ) -> Self {
        Self {
            auth,
            http_client,
            ingestion_endpoint,
            cached: Arc::new(RwLock::new(None)),
        }
    }

    /// Returns cached resources, refreshing them if stale or absent.
    pub(super) async fn get_resources(&self) -> crate::Result<IngestionResources> {
        // Fast path: check if cached resources are still fresh.
        {
            let cached = self.cached.read().await;
            if let Some(ref resources) = *cached {
                if !resources.is_stale() {
                    return Ok(resources.clone());
                }
            }
        }

        // Slow path: refresh resources.
        let resources = self.load_resources().await?;
        {
            let mut cached = self.cached.write().await;
            *cached = Some(resources.clone());
        }
        Ok(resources)
    }

    async fn load_resources(&self) -> crate::Result<IngestionResources> {
        // Step 1: Get ingestion resources (blob + queue endpoints)
        let resources_response = self
            .execute_mgmt_command(".get ingestion resources")
            .await?;
        let (blob_endpoints, queue_endpoints) = parse_storage_resources(&resources_response)?;

        if blob_endpoints.is_empty() {
            return Err("No blob storage endpoints returned by ADX".into());
        }
        if queue_endpoints.is_empty() {
            return Err("No queue endpoints returned by ADX".into());
        }

        // Step 2: Get identity token
        let identity_response = self
            .execute_mgmt_command(".get kusto identity token")
            .await?;
        let identity_token = parse_identity_token(&identity_response)?;

        info!(
            message = "Loaded ADX ingestion resources.",
            blob_count = blob_endpoints.len(),
            queue_count = queue_endpoints.len(),
        );

        Ok(IngestionResources {
            blob_endpoints,
            queue_endpoints,
            identity_token,
            loaded_at: Instant::now(),
        })
    }

    /// Executes a CSL management command against the ingestion endpoint.
    async fn execute_mgmt_command(&self, csl: &str) -> crate::Result<String> {
        let token = self.auth.get_token().await?;
        let mgmt_uri = format!(
            "{}{}",
            self.ingestion_endpoint.trim_end_matches('/'),
            MGMT_URI_PATH
        );

        let body = serde_json::json!({
            "csl": csl,
            "db": "NetDefaultDB"
        });
        let body_bytes = Bytes::from(serde_json::to_vec(&body)?);

        let request = Request::post(&mgmt_uri)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(hyper::Body::from(body_bytes))?;

        let response = self.http_client.send(request).await?;
        let status = response.status();
        let body = http_body::Body::collect(response.into_body())
            .await?
            .to_bytes();
        let body_str = String::from_utf8_lossy(&body).to_string();

        if status.is_success() {
            Ok(body_str)
        } else {
            Err(format!(
                "ADX management command '{}' failed: HTTP {} - {}",
                csl,
                status,
                &body_str[..body_str.len().min(500)]
            )
            .into())
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parses the `.get ingestion resources` response into blob and queue endpoints.
///
/// Response format (JSON):
/// ```json
/// {
///   "Tables": [{
///     "Rows": [
///       ["TempStorage", "https://host.blob.core.windows.net/container?sas"],
///       ["SecuredReadyForAggregationQueue", "https://host.queue.core.windows.net/queue?sas"],
///       ...
///     ]
///   }]
/// }
/// ```
fn parse_storage_resources(
    response: &str,
) -> crate::Result<(Vec<StorageEndpoint>, Vec<StorageEndpoint>)> {
    let json: Value = serde_json::from_str(response)
        .map_err(|e| format!("Failed to parse ingestion resources response: {e}"))?;

    let mut blob_endpoints = Vec::new();
    let mut queue_endpoints = Vec::new();

    let rows = json
        .get("Tables")
        .and_then(|t| t.get(0))
        .and_then(|t| t.get("Rows"))
        .and_then(|r| r.as_array())
        .ok_or("Unexpected ingestion resources response format")?;

    for row in rows {
        let row_arr = match row.as_array() {
            Some(arr) if arr.len() >= 2 => arr,
            _ => continue,
        };

        let resource_type = row_arr[0].as_str().unwrap_or("");
        let resource_uri = row_arr[1].as_str().unwrap_or("");

        if resource_uri.is_empty() {
            continue;
        }

        let endpoint = match parse_sas_url(resource_uri) {
            Some(ep) => ep,
            None => continue,
        };

        match resource_type {
            "TempStorage" => blob_endpoints.push(endpoint),
            "SecuredReadyForAggregationQueue" => queue_endpoints.push(endpoint),
            _ => {} // Ignore other resource types (e.g. FailedIngestionsQueue)
        }
    }

    Ok((blob_endpoints, queue_endpoints))
}

/// Parses the `.get kusto identity token` response.
///
/// Response format (JSON):
/// ```json
/// {
///   "Tables": [{
///     "Rows": [["<identity_token>"]]
///   }]
/// }
/// ```
fn parse_identity_token(response: &str) -> crate::Result<String> {
    let json: Value = serde_json::from_str(response)
        .map_err(|e| format!("Failed to parse identity token response: {e}"))?;

    let token = json
        .get("Tables")
        .and_then(|t| t.get(0))
        .and_then(|t| t.get("Rows"))
        .and_then(|r| r.as_array())
        .and_then(|rows| rows.first())
        .and_then(|row| row.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .ok_or("Failed to extract identity token from ADX response")?;

    Ok(token.to_string())
}

/// Splits a SAS URL into base URL and SAS token query string.
///
/// Input:  `https://host.blob.core.windows.net/container?sv=...&sig=...`
/// Output: `StorageEndpoint { base_url: "https://...", sas_token: "sv=...&sig=..." }`
fn parse_sas_url(url: &str) -> Option<StorageEndpoint> {
    let (base_url, sas_token) = url.split_once('?')?;
    Some(StorageEndpoint {
        base_url: base_url.to_string(),
        sas_token: sas_token.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sas_url_splits_correctly() {
        let ep =
            parse_sas_url("https://host.blob.core.windows.net/container?sv=2019&sig=abc").unwrap();
        assert_eq!(ep.base_url, "https://host.blob.core.windows.net/container");
        assert_eq!(ep.sas_token, "sv=2019&sig=abc");
    }

    #[test]
    fn parse_sas_url_returns_none_without_query() {
        assert!(parse_sas_url("https://host.blob.core.windows.net/container").is_none());
    }

    #[test]
    fn parse_storage_resources_extracts_endpoints() {
        let response = r#"{
            "Tables": [{
                "TableName": "Table_0",
                "Columns": [],
                "Rows": [
                    ["TempStorage", "https://blob1.blob.core.windows.net/c1?sas1"],
                    ["TempStorage", "https://blob2.blob.core.windows.net/c2?sas2"],
                    ["SecuredReadyForAggregationQueue", "https://queue1.queue.core.windows.net/q1?sas3"],
                    ["SuccessfulIngestionsQueue", "https://other.queue.core.windows.net/q2?sas4"],
                    ["FailedIngestionsQueue", "https://other2.queue.core.windows.net/q3?sas5"]
                ]
            }]
        }"#;

        let (blobs, queues) = parse_storage_resources(response).unwrap();
        assert_eq!(blobs.len(), 2);
        assert_eq!(queues.len(), 1);
        assert_eq!(blobs[0].base_url, "https://blob1.blob.core.windows.net/c1");
        assert_eq!(blobs[0].sas_token, "sas1");
        assert_eq!(
            queues[0].base_url,
            "https://queue1.queue.core.windows.net/q1"
        );
        assert_eq!(queues[0].sas_token, "sas3");
    }

    #[test]
    fn parse_identity_token_extracts_token() {
        let response = r#"{
            "Tables": [{
                "TableName": "Table_0",
                "Columns": [{"ColumnName": "AuthorizationContext"}],
                "Rows": [["my_identity_token_value"]]
            }]
        }"#;

        let token = parse_identity_token(response).unwrap();
        assert_eq!(token, "my_identity_token_value");
    }
}
