use crate::{
    http::{Auth, HttpClient},
    internal_events::EndpointBytesSent,
    sinks::util::Compression,
};
use bytes::Bytes;
use http::{
    Method, Response, StatusCode, Uri,
    header::{CONTENT_LENGTH, CONTENT_TYPE, EXPECT},
};
use http_body::{Body as _, Collected};
use hyper::{Body, Request};
use serde_json::Value;
use snafu::Snafu;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::SystemTime,
};
use tracing::debug;
use uuid::Uuid;

/// Content-Type header value for Doris Stream Load requests.
/// Doris expects plain text with UTF-8 encoding for JSON data.
const DORIS_CONTENT_TYPE: &str = "text/plain;charset=utf-8";

/// Expect header value for Doris Stream Load requests.
/// The "100-continue" mechanism allows the client to wait for server acknowledgment
/// before sending the request body, which is required by Doris Stream Load protocol.
const DORIS_EXPECT_HEADER: &str = "100-continue";

/// Thread-safe version of the DorisSinkClient, wrapped in an Arc
pub type ThreadSafeDorisSinkClient = Arc<DorisSinkClient>;

/// DorisSinkClient handles the HTTP communication with Doris server
/// This client is thread-safe by design
#[derive(Clone, Debug)]
pub struct DorisSinkClient {
    http_client: HttpClient,
    base_url: String,
    auth: Option<Auth>,
    compression: Compression,
    label_prefix: String,
    headers: Arc<HashMap<String, String>>,
}

impl DorisSinkClient {
    pub async fn new(
        http_client: HttpClient,
        base_url: String,
        auth: Option<Auth>,
        compression: Compression,
        label_prefix: String,
        req_headers: HashMap<String, String>,
    ) -> Self {
        // Store custom headers (basic headers like Content-Type and Expect
        // are set directly in the request builder)
        let headers = req_headers;

        Self {
            http_client,
            base_url,
            auth,
            compression,
            label_prefix,
            headers: Arc::new(headers),
        }
    }

    /// Converts a DorisSinkClient into a thread-safe version
    pub fn into_thread_safe(self) -> ThreadSafeDorisSinkClient {
        Arc::new(self)
    }

    /// Generate a unique label for the stream load
    fn generate_label(&self, database: &str, table: &str) -> String {
        format!(
            "{}_{}_{}_{}_{}",
            self.label_prefix,
            database,
            table,
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            Uuid::new_v4()
        )
    }

    /// Build a request for the Doris stream load
    async fn build_request(
        &self,
        database: &str,
        table: &str,
        payload: &Bytes,
        redirect_url: Option<&str>,
    ) -> Result<Request<Body>, crate::Error> {
        let label = self.generate_label(database, table);

        let uri = if let Some(redirect_url) = redirect_url {
            debug!(%redirect_url, "Using redirect URL.");
            redirect_url.parse::<Uri>().map_err(|source| {
                debug!(
                    message = "Failed to parse redirect URI.",
                    %source,
                    %redirect_url
                );
                StreamLoadError::InvalidRedirectUri { source }
            })?
        } else {
            // Build original URL
            let stream_load_url =
                format!("{}/api/{}/{}/_stream_load", self.base_url, database, table);

            stream_load_url.parse::<Uri>().map_err(|source| {
                debug!(
                    message = "Failed to parse URI.",
                    %source,
                    url = %stream_load_url
                );
                StreamLoadError::InvalidStreamLoadUri { source }
            })?
        };

        debug!(%uri, %label, "Building request.");

        let mut builder = Request::builder()
            .method(Method::PUT)
            .uri(uri.clone())
            .header(CONTENT_LENGTH, payload.len())
            .header(CONTENT_TYPE, DORIS_CONTENT_TYPE)
            .header(EXPECT, DORIS_EXPECT_HEADER)
            .header("label", &label);

        // Add compression headers if needed
        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        if let Some(ae) = self.compression.accept_encoding() {
            builder = builder.header("Accept-Encoding", ae);
        }

        // Add custom headers
        for (header, value) in self.headers.as_ref() {
            builder = builder.header(&header[..], &value[..]);
        }

        let body = Body::from(payload.clone());
        let mut request = builder.body(body).map_err(|source| {
            debug!(
                message = "Failed to build HTTP request.",
                %source,
                uri = %uri
            );
            StreamLoadError::BuildRequest { source }
        })?;

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        debug!(
            message = "Request built successfully.",
            method = %request.method(),
            uri = %request.uri(),
            headers_count = request.headers().len()
        );

        Ok(request)
    }

    /// Handle redirects and send the HTTP request to Doris
    /// Returns the HTTP response and event status
    pub async fn send_stream_load(
        &self,
        database: String,
        table: String,
        payload: Bytes,
    ) -> Result<DorisStreamLoadResponse, crate::Error> {
        // Track visited URLs to prevent redirect loops
        let mut visited_urls = HashSet::new();
        let mut redirect_count = 0;
        // Doris Stream Load typically redirects once (FE -> BE), but we allow up to 3
        // redirects to handle potential multi-hop scenarios while preventing infinite loops.
        const MAX_REDIRECTS: u8 = 3;

        let payload_ref = &payload;

        // Build and send initial request
        let request = self
            .build_request(&database, &table, payload_ref, None)
            .await?;
        let endpoint = request.uri().to_string();
        let byte_size = payload.len();

        let mut response = self.http_client.send(request).await?;
        let mut status = response.status();

        // Track protocol for metrics
        let protocol = if endpoint.starts_with("https://") {
            "https"
        } else {
            "http"
        };

        // Handle redirect loop
        while (status == StatusCode::TEMPORARY_REDIRECT
            || status == StatusCode::PERMANENT_REDIRECT
            || status == StatusCode::FOUND)
            && redirect_count < MAX_REDIRECTS
        {
            // Get redirect location
            if let Some(location) = response.headers().get(http::header::LOCATION) {
                if let Ok(location_str) = location.to_str() {
                    debug!(
                        message = "Following redirect.",
                        status = %status,
                        to = %location_str,
                        redirect_count = redirect_count + 1
                    );

                    // Check for redirect loop
                    if !visited_urls.insert(location_str.to_string()) {
                        return Err(StreamLoadError::RedirectLoop.into());
                    }

                    // Build and send redirect request
                    let redirect_req = self
                        .build_request(&database, &table, payload_ref, Some(location_str))
                        .await?;

                    response = self.http_client.send(redirect_req).await?;
                    status = response.status();

                    // Increment redirect counter
                    redirect_count += 1;

                    debug!(
                        message = "Received response after redirect.",
                        new_status = %status,
                        redirect_count = redirect_count
                    );
                } else {
                    return Err(StreamLoadError::InvalidLocationHeader.into());
                }
            } else {
                return Err(StreamLoadError::MissingLocationHeader.into());
            }
        }

        // Check if maximum redirects exceeded
        if redirect_count >= MAX_REDIRECTS {
            return Err(StreamLoadError::MaxRedirectsExceeded { max: MAX_REDIRECTS }.into());
        }

        // Log endpoint bytes sent metric
        emit!(EndpointBytesSent {
            byte_size,
            protocol,
            endpoint: endpoint.as_str(),
        });

        // Extract response body
        let (parts, body) = response.into_parts();
        let body_bytes = body
            .collect()
            .await
            .map(Collected::to_bytes)
            .map_err(|source| StreamLoadError::ReadResponseBody { source })?;

        let status = parts.status;

        let response_json = serde_json::from_slice::<Value>(&body_bytes)
            .map_err(|source| StreamLoadError::ParseResponseJson { source })?;

        let stream_load_status =
            if let Some(status_str) = response_json.get("Status").and_then(|v| v.as_str()) {
                if status_str.to_lowercase() == "success" {
                    StreamLoadStatus::Successful
                } else {
                    StreamLoadStatus::Failure
                }
            } else {
                StreamLoadStatus::Failure
            };

        Ok(DorisStreamLoadResponse {
            http_status_code: status,
            stream_load_status,
            response: Response::from_parts(parts, body_bytes),
            response_json,
        })
    }

    pub async fn healthcheck_fenode(&self, endpoint: String) -> crate::Result<()> {
        // Use Doris bootstrap API endpoint for health check, GET method
        let query_path = "/api/bootstrap";
        let endpoint_str = endpoint.trim_end_matches('/');
        let uri_str = format!("{}{}", endpoint_str, query_path);

        let uri = uri_str.parse::<Uri>().map_err(|source| {
            debug!(
                message = "Failed to parse health check URI.",
                %source,
                url = %uri_str
            );
            HealthCheckError::InvalidUri { source }
        })?;

        debug!(
            message = "Sending health check request to Doris FE node.",
            uri = %uri
        );

        let mut request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .map_err(|source| HealthCheckError::HealthCheckBuildRequest { source })?;

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        let response = self.http_client.send(request).await?;
        let status = response.status();

        let (_, body) = response.into_parts();
        let body_bytes = body
            .collect()
            .await
            .map(Collected::to_bytes)
            .map_err(|source| HealthCheckError::HealthCheckReadResponseBody { source })?;

        if status.is_success() {
            // Parse the response JSON
            match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                Ok(json) => {
                    // Check if the msg field is "success"
                    if let Some(msg) = json.get("msg").and_then(|m| m.as_str()) {
                        if msg.to_lowercase() == "success" {
                            debug!(
                                message = "Doris FE node is healthy.",
                                node = %endpoint
                            );
                            return Ok(());
                        } else {
                            debug!(
                                message = "Doris FE node returned non-success message.",
                                node = %endpoint,
                                message = %msg
                            );
                            return Err(HealthCheckError::HealthCheckFailed {
                                message: msg.to_string(),
                            }
                            .into());
                        }
                    }
                }
                Err(source) => {
                    return Err(HealthCheckError::HealthCheckParseResponse { source }.into());
                }
            }
        }

        debug!(
            message = "Doris FE node health check failed.",
            node = %endpoint,
            status = %status
        );

        Err(HealthCheckError::HealthCheckFailed {
            message: format!("HTTP status: {}", status),
        }
        .into())
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct DorisStreamLoadResponse {
    pub http_status_code: StatusCode,
    pub stream_load_status: StreamLoadStatus,
    pub response: Response<Bytes>,
    pub response_json: Value,
}

impl Clone for DorisStreamLoadResponse {
    fn clone(&self) -> Self {
        let cloned_response = Response::builder()
            .status(self.http_status_code)
            .body(self.response.body().clone())
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Bytes::new())
                    .unwrap()
            });

        Self {
            http_status_code: self.http_status_code,
            stream_load_status: self.stream_load_status.clone(),
            response: cloned_response,
            response_json: self.response_json.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamLoadStatus {
    Successful,
    Failure,
}
impl std::fmt::Display for StreamLoadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Successful => write!(f, "Successful"),
            Self::Failure => write!(f, "Failure"),
        }
    }
}

impl From<StreamLoadStatus> for vector_common::finalization::EventStatus {
    fn from(status: StreamLoadStatus) -> Self {
        match status {
            StreamLoadStatus::Successful => vector_common::finalization::EventStatus::Delivered,
            StreamLoadStatus::Failure => vector_common::finalization::EventStatus::Errored,
        }
    }
}

/// Errors that can occur during Doris Stream Load operations.
#[derive(Debug, Snafu)]
pub enum StreamLoadError {
    #[snafu(display("Invalid redirect URI: {}", source))]
    InvalidRedirectUri { source: http::uri::InvalidUri },

    #[snafu(display("Invalid stream load URI: {}", source))]
    InvalidStreamLoadUri { source: http::uri::InvalidUri },

    #[snafu(display("Detected redirect loop"))]
    RedirectLoop,

    #[snafu(display("Invalid Location header in redirect response"))]
    InvalidLocationHeader,

    #[snafu(display("Missing Location header in redirect response"))]
    MissingLocationHeader,

    #[snafu(display("Exceeded maximum number of redirects ({})", max))]
    MaxRedirectsExceeded { max: u8 },

    #[snafu(display("Failed to build request: {}", source))]
    BuildRequest { source: http::Error },

    #[snafu(display("Failed to read response body: {}", source))]
    ReadResponseBody { source: hyper::Error },

    #[snafu(display("Failed to parse response JSON: {}", source))]
    ParseResponseJson { source: serde_json::Error },
}

/// Errors that can occur during Doris health check operations.
#[derive(Debug, Snafu)]
pub enum HealthCheckError {
    #[snafu(display("Invalid health check URI: {}", source))]
    InvalidUri { source: http::uri::InvalidUri },

    #[snafu(display("Failed to build health check request: {}", source))]
    HealthCheckBuildRequest { source: http::Error },

    #[snafu(display("Failed to read health check response body: {}", source))]
    HealthCheckReadResponseBody { source: hyper::Error },

    #[snafu(display("Failed to parse health check response: {}", source))]
    HealthCheckParseResponse { source: serde_json::Error },

    #[snafu(display("Doris health check failed: {}", message))]
    HealthCheckFailed { message: String },
}
