use crate::{
    http::{Auth, client_v1::HttpClient},
    internal_events::EndpointBytesSent,
    sinks::util::Compression,
};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// Percent-encoding set for URL path segments (RFC 3986 unreserved chars are left unencoded).
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
use bytes::Bytes;
use http::{
    Method, Request, Response, StatusCode, Uri,
    header::{CONTENT_LENGTH, CONTENT_TYPE, EXPECT, LOCATION},
};
use http_body_util::{BodyExt as _, Collected};
use serde_json::Value;
use snafu::Snafu;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::SystemTime,
};
use tracing::debug;
use uuid::Uuid;
use vector_http::{RequestBody, empty_body, full_body};

/// Content-Type header value for Doris Stream Load requests.
/// Doris expects plain text with UTF-8 encoding for JSON data.
const DORIS_CONTENT_TYPE: &str = "text/plain;charset=utf-8";

/// Expect header value for Doris Stream Load requests.
/// The "100-continue" mechanism allows the client to wait for server acknowledgment
/// before sending the request body, which is required by Doris Stream Load protocol.
const DORIS_EXPECT_HEADER: &str = "100-continue";

/// Group commit header name for Doris Stream Load.
const GROUP_COMMIT_HEADER: &str = "group_commit";

/// Group commit sync mode - multiple imports merged into one transaction,
/// returns after transaction commit. Data is visible immediately after import.
const GROUP_COMMIT_SYNC_MODE: &str = "sync_mode";

/// Group commit async mode - data written to WAL first, returns immediately.
/// Data is visible after async commit based on `group_commit_interval`.
const GROUP_COMMIT_ASYNC_MODE: &str = "async_mode";

/// Thread-safe version of the `DorisSinkClient`, wrapped in an Arc
pub type ThreadSafeDorisSinkClient = Arc<DorisSinkClient>;

/// `DorisSinkClient` handles the HTTP communication with Doris server
/// This client is thread-safe by design
#[derive(Clone, Debug)]
pub struct DorisSinkClient {
    http_client: HttpClient,
    base_url: Uri,
    auth: Option<Auth>,
    compression: Compression,
    label_prefix: String,
    headers: Arc<HashMap<String, String>>,
}

impl DorisSinkClient {
    pub fn new(
        http_client: HttpClient,
        base_url: Uri,
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

    /// Converts a `DorisSinkClient` into a thread-safe version
    pub fn into_thread_safe(self) -> ThreadSafeDorisSinkClient {
        Arc::new(self)
    }

    /// Generate a unique label for the stream load
    fn generate_label(&self, database: &str, table: &str) -> String {
        format!(
            "{}_{database}_{table}_{}_{}",
            self.label_prefix,
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            Uuid::new_v4()
        )
    }

    /// Check if group commit is enabled in the custom headers
    /// Group commit has three modes:
    /// - `off_mode`: disabled, label is required
    /// - `sync_mode`: enabled, label should be skipped
    /// - `async_mode`: enabled, label should be skipped
    fn is_group_commit_enabled(&self) -> bool {
        self.headers.iter().any(|(k, v)| {
            k.eq_ignore_ascii_case(GROUP_COMMIT_HEADER)
                && (v.eq_ignore_ascii_case(GROUP_COMMIT_SYNC_MODE)
                    || v.eq_ignore_ascii_case(GROUP_COMMIT_ASYNC_MODE))
        })
    }

    /// Build a request for the Doris stream load
    fn build_request(
        &self,
        database: &str,
        table: &str,
        payload: &Bytes,
        redirect_url: Option<&str>,
    ) -> Result<Request<RequestBody>, crate::Error> {
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
            // Build original URL using Uri components to avoid trailing slash issues
            let scheme = self.base_url.scheme_str().unwrap_or("http");
            let authority = self
                .base_url
                .authority()
                .map_or("", http::uri::Authority::as_str);
            let encoded_database = utf8_percent_encode(database, PATH_SEGMENT);
            let encoded_table = utf8_percent_encode(table, PATH_SEGMENT);
            let stream_load_url = format!(
                "{scheme}://{authority}/api/{encoded_database}/{encoded_table}/_stream_load"
            );

            stream_load_url.parse::<Uri>().map_err(|source| {
                debug!(
                    message = "Failed to parse URI.",
                    %source,
                    url = %stream_load_url
                );
                StreamLoadError::InvalidStreamLoadUri { source }
            })?
        };

        let mut builder = Request::builder()
            .method(Method::PUT)
            .uri(uri.clone())
            .header(CONTENT_LENGTH, payload.len())
            .header(CONTENT_TYPE, DORIS_CONTENT_TYPE)
            .header(EXPECT, DORIS_EXPECT_HEADER);

        // Only set label when group commit is not enabled
        // When group commit is enabled, Doris server ignores the label anyway
        // as multiple requests are merged into a single transaction
        if self.is_group_commit_enabled() {
            debug!(%uri, "Building request with group commit enabled (no label).");
        } else {
            let label = self.generate_label(database, table);
            debug!(%uri, %label, "Building request.");
            builder = builder.header("label", &label);
        }

        // Add compression headers if needed
        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        if let Some(ae) = self.compression.accept_encoding() {
            builder = builder.header("Accept-Encoding", ae);
        }

        // Add custom headers
        for (header, value) in self.headers.as_ref() {
            builder = builder.header(header.as_str(), value.as_str());
        }

        // Cloning `Bytes` only bumps a reference count; redirects replay the same payload.
        let mut request = builder.body(payload.clone()).map_err(|source| {
            debug!(
                message = "Failed to build HTTP request.",
                %source,
                uri = %uri
            );
            StreamLoadError::BuildRequest { source }
        })?;

        if let Some(auth) = &self.auth {
            auth.apply_v1(&mut request);
        }

        debug!(
            message = "Request built successfully.",
            method = %request.method(),
            uri = %request.uri(),
            headers_count = request.headers().len()
        );

        Ok(request.map(full_body))
    }

    /// Handle redirects and send the HTTP request to Doris
    /// Returns the HTTP response and event status
    pub async fn send_stream_load(
        &self,
        database: String,
        table: String,
        payload: Bytes,
    ) -> Result<DorisStreamLoadResponse, crate::Error> {
        // Doris Stream Load typically redirects once (FE -> BE), but we allow up to 3
        // redirects to handle potential multi-hop scenarios while preventing infinite loops.
        const MAX_REDIRECTS: u8 = 3;

        // Track visited URLs to prevent redirect loops
        let mut visited_urls = HashSet::new();
        let mut redirect_count = 0;

        let payload_ref = &payload;

        // Build and send initial request
        let request = self.build_request(&database, &table, payload_ref, None)?;
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
            if let Some(location) = response.headers().get(LOCATION) {
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
                    let redirect_req =
                        self.build_request(&database, &table, payload_ref, Some(location_str))?;

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

    pub async fn healthcheck_fenode(&self, endpoint: &Uri) -> crate::Result<()> {
        // Use Doris bootstrap API endpoint for health check, GET method
        let scheme = endpoint.scheme_str().unwrap_or("http");
        let authority = endpoint
            .authority()
            .map_or("", http::uri::Authority::as_str);
        let uri_str = format!("{scheme}://{authority}/api/bootstrap");

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
            .body(empty_body())
            .map_err(|source| HealthCheckError::HealthCheckBuildRequest { source })?;

        if let Some(auth) = &self.auth {
            auth.apply_v1(&mut request);
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
                        }
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
            message: format!("HTTP status: {status}"),
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
    ReadResponseBody { source: hyper_1::Error },

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
    HealthCheckReadResponseBody { source: hyper_1::Error },

    #[snafu(display("Failed to parse health check response: {}", source))]
    HealthCheckParseResponse { source: serde_json::Error },

    #[snafu(display("Doris health check failed: {}", message))]
    HealthCheckFailed { message: String },
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use hyper::{
        Server,
        body::HttpBody as _,
        service::{make_service_fn, service_fn},
    };
    use vector_common::sensitive_string::SensitiveString;

    use super::*;
    use crate::{config::ProxyConfig, test_util::addr::next_addr, tls::MaybeTlsSettings};

    #[tokio::test]
    async fn stream_load_replays_payload_and_auth_across_redirect() {
        let (_guard, addr) = next_addr();
        let received = Arc::new(Mutex::new(Vec::new()));
        let server_received = Arc::clone(&received);
        let service = make_service_fn(move |_| {
            let received = Arc::clone(&server_received);
            async move {
                Ok::<_, crate::Error>(service_fn(move |request: hyper::Request<hyper::Body>| {
                    let received = Arc::clone(&received);
                    async move {
                        let (parts, body) = request.into_parts();
                        let body = body.collect().await?.to_bytes();
                        let response = if parts.uri.path() == "/be/_stream_load" {
                            hyper::Response::new(hyper::Body::from(r#"{"Status":"Success"}"#))
                        } else {
                            hyper::Response::builder()
                                .status(hyper::StatusCode::TEMPORARY_REDIRECT)
                                .header(
                                    hyper::header::LOCATION,
                                    format!("http://{addr}/be/_stream_load?k=a%2Fb"),
                                )
                                .body(hyper::Body::empty())?
                        };
                        received.lock().unwrap().push((parts, body));
                        Ok::<_, crate::Error>(response)
                    }
                }))
            }
        });
        tokio::spawn(Server::bind(&addr).serve(service));

        let http_client = HttpClient::new(
            MaybeTlsSettings::from_config(None, false).unwrap(),
            &ProxyConfig::default(),
        )
        .unwrap();
        let client = DorisSinkClient::new(
            http_client,
            format!("http://{addr}").parse().unwrap(),
            Some(Auth::Basic {
                user: "user".to_string(),
                password: SensitiveString::from("pass".to_string()),
            }),
            Compression::None,
            "vector".to_string(),
            HashMap::new(),
        );
        let payload = Bytes::from_static(b"{\"message\":\"hello\"}\n");

        let response = client
            .send_stream_load("my db".to_string(), "t/bl".to_string(), payload.clone())
            .await
            .unwrap();

        assert_eq!(response.http_status_code, StatusCode::OK);
        assert_eq!(response.stream_load_status, StreamLoadStatus::Successful);
        let received = received.lock().unwrap();
        let targets = received
            .iter()
            .map(|(parts, _)| parts.uri.path_and_query().unwrap().as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            targets,
            [
                "/api/my%20db/t%2Fbl/_stream_load",
                "/be/_stream_load?k=a%2Fb"
            ]
        );
        for (parts, body) in received.iter() {
            assert_eq!(parts.method, hyper::Method::PUT);
            assert_eq!(body, &payload);
            assert_eq!(parts.headers["authorization"], "Basic dXNlcjpwYXNz");
            assert_eq!(parts.headers["expect"], DORIS_EXPECT_HEADER);
            assert_eq!(parts.headers["content-type"], DORIS_CONTENT_TYPE);
            assert_eq!(
                parts.headers["content-length"],
                payload.len().to_string().as_str()
            );
            assert!(parts.headers.contains_key("label"));
        }
    }
}
