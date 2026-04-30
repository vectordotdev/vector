//! Service implementation for the `azure_data_explorer` sink.
//!
//! Implements **streaming ingestion** via the Kusto REST API:
//! `POST /v1/rest/ingest/{database}/{table}?streamFormat=MultiJSON[&mappingName=...]`
//!
//! See: <https://learn.microsoft.com/en-us/azure/data-explorer/kusto/api/rest/streaming-ingest>

use std::{
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use http::Request;
use tower::Service;
use url::Url;
use uuid::Uuid;

use super::auth::AzureDataExplorerAuth;
use crate::{
    http::HttpClient,
    internal_events::EndpointBytesSent,
    sinks::{
        prelude::*,
        util::{
            buffer::compression::Compression,
            http::{HttpRequest, HttpResponse},
            uri::protocol_endpoint,
        },
    },
};

/// Configuration for streaming ingest, shared across all clones of the service.
#[derive(Clone, Debug)]
pub(super) struct StreamingIngestConfig {
    pub ingestion_endpoint: String,
    pub database: String,
    pub table: String,
    pub mapping_reference: Option<String>,
    pub compression: Compression,
}

/// A Tower `Service` that performs **streaming ingestion** to Azure Data Explorer.
///
/// Each `call()` issues one authenticated POST with the batch body to
/// `/v1/rest/ingest/{database}/{table}` on the configured ingestion endpoint.
pub(super) struct AzureDataExplorerService {
    http_client: HttpClient,
    auth: AzureDataExplorerAuth,
    config: Arc<StreamingIngestConfig>,
}

impl AzureDataExplorerService {
    pub(super) fn new(
        http_client: HttpClient,
        auth: AzureDataExplorerAuth,
        config: StreamingIngestConfig,
    ) -> Self {
        Self {
            http_client,
            auth,
            config: Arc::new(config),
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
        let auth = self.auth.clone();
        let config = Arc::clone(&self.config);

        let metadata = std::mem::take(request.metadata_mut());
        let raw_byte_size = metadata.request_encoded_size();
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();
        let payload = request.take_payload();

        Box::pin(async move {
            let ingest_uri = build_streaming_ingest_url(&config)?;

            let token = auth.get_token().await?;

            debug!(
                message = "Sending streaming ingest request to Azure Data Explorer.",
                uri = %ingest_uri,
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
                debug!(message = "Streaming ingest completed successfully.", status = %status);
                emit!(EndpointBytesSent {
                    byte_size: raw_byte_size,
                    protocol: &protocol,
                    endpoint: &endpoint,
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
        })
    }
}

impl Clone for AzureDataExplorerService {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            auth: self.auth.clone(),
            config: Arc::clone(&self.config),
        }
    }
}

/// Builds `POST {ingestion_endpoint}/v1/rest/ingest/{database}/{table}?streamFormat=MultiJSON...`
fn build_streaming_ingest_url(config: &StreamingIngestConfig) -> crate::Result<Url> {
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
        .push(config.table.as_str());

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_ingest_url_basic() {
        let config = StreamingIngestConfig {
            ingestion_endpoint: "https://ingest-mycluster.eastus.kusto.windows.net".to_string(),
            database: "MyDb".to_string(),
            table: "MyTable".to_string(),
            mapping_reference: None,
            compression: Compression::None,
        };
        let u = build_streaming_ingest_url(&config).unwrap();
        assert_eq!(u.path(), "/v1/rest/ingest/MyDb/MyTable");
        let q: std::collections::HashMap<String, String> = u.query_pairs().into_owned().collect();
        assert_eq!(q.get("streamFormat").map(String::as_str), Some("MultiJSON"));
        assert!(!q.contains_key("mappingName"));
    }

    #[test]
    fn streaming_ingest_url_with_mapping() {
        let config = StreamingIngestConfig {
            ingestion_endpoint: "https://ingest.example.com/".to_string(),
            database: "db".to_string(),
            table: "tbl".to_string(),
            mapping_reference: Some("my_map".to_string()),
            compression: Compression::None,
        };
        let u = build_streaming_ingest_url(&config).unwrap();
        assert!(u.as_str().contains("mappingName=my_map"));
        assert!(u.as_str().contains("streamFormat=MultiJSON"));
    }
}
