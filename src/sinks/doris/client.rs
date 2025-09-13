use crate::{
    http::{Auth, HttpClient},
    internal_events::EndpointBytesSent,
    sinks::util::Compression,
};
use bytes::Bytes;
use http::{
    Method, Response, StatusCode, Uri,
    header::{CONTENT_LENGTH, CONTENT_TYPE},
};
use hyper::{Body, Request};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::SystemTime,
};
use tracing::debug;
use uuid::Uuid;

/// Thread-safe version of the DorisSinkClient, wrapped in an Arc
pub type ThreadSafeDorisSinkClient = Arc<DorisSinkClient>;

/// DorisSinkClient handles the HTTP communication with Doris server
/// This client is thread-safe by design
#[derive(Clone, Debug)]
pub struct DorisSinkClient {
    http_client: HttpClient,
    base_url: String, // http://10.16.10.6:8630
    auth: Option<Auth>,
    compression: Compression,
    label_prefix: String,
    headers: Arc<HashMap<String, String>>,
}

// Explicitly implement Send and Sync for DorisSinkClient
// This is safe because all internal fields implement Send + Sync
unsafe impl Send for DorisSinkClient {}
unsafe impl Sync for DorisSinkClient {}

impl DorisSinkClient {
    pub async fn new(
        http_client: HttpClient,
        base_url: String,
        auth: Option<Auth>,
        compression: Compression,
        label_prefix: String,
        req_headers: HashMap<String, String>,
    ) -> Self {
        // Create and set headers
        let mut headers = HashMap::new();
        // Basic headers
        headers.insert("Expect".to_string(), "100-continue".to_string());
        headers.insert(
            "Content-Type".to_string(),
            "text/plain;charset=utf-8".to_string(),
        );

        // Add custom headers
        for (k, v) in &req_headers {
            headers.insert(k.clone(), v.clone());
        }

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
            debug!(
                message = "Using redirect URL",
                redirect_url = %redirect_url
            );
            redirect_url.parse::<Uri>().map_err(|error| {
                debug!(
                    message = "Failed to parse redirect URI.",
                    %error,
                    redirect_url = %redirect_url
                );
                crate::Error::from(format!("Invalid redirect URI: {}", error))
            })?
        } else {
            // Build original URL
            let stream_load_url =
                format!("{}/api/{}/{}/_stream_load", self.base_url, database, table);

            stream_load_url.parse::<Uri>().map_err(|error| {
                debug!(
                    message = "Failed to parse URI.",
                    %error,
                    url = %stream_load_url
                );
                crate::Error::from(format!("Invalid URI: {}", error))
            })?
        };

        debug!(
            message = "Building request",
            uri = %uri,
            label = %label
        );

        let mut builder = Request::builder()
            .method(Method::PUT)
            .uri(uri.clone())
            .header(CONTENT_LENGTH, payload.len())
            .header(CONTENT_TYPE, "text/plain;charset=utf-8")
            .header("Expect", "100-continue")
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
        let mut request = builder.body(body).map_err(|error| {
            debug!(
                message = "Failed to build HTTP request.",
                %error,
                uri = %uri
            );
            crate::Error::from(format!("Failed to build request: {}", error))
        })?;

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        debug!(
            message = "Request built successfully",
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
                        message = "Following redirect",
                        status = %status,
                        to = %location_str,
                        redirect_count = redirect_count + 1
                    );

                    // Check for redirect loop
                    if !visited_urls.insert(location_str.to_string()) {
                        return Err(crate::Error::from("Detected redirect loop"));
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
                        message = "Received response after redirect",
                        new_status = %status,
                        redirect_count = redirect_count
                    );
                } else {
                    return Err(crate::Error::from(
                        "Invalid Location header in redirect response",
                    ));
                }
            } else {
                return Err(crate::Error::from(
                    "Missing Location header in redirect response",
                ));
            }
        }

        // Check if maximum redirects exceeded
        if redirect_count >= MAX_REDIRECTS {
            return Err(crate::Error::from(format!(
                "Exceeded maximum number of redirects ({})",
                MAX_REDIRECTS
            )));
        }

        // Log endpoint bytes sent metric
        emit!(EndpointBytesSent {
            byte_size,
            protocol,
            endpoint: endpoint.as_str(),
        });

        // Extract response body
        let (parts, body) = response.into_parts();
        let body_bytes = hyper::body::to_bytes(body)
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        let status = parts.status;

        let response_json = serde_json::from_slice::<Value>(&body_bytes)
            .map_err(|e| format!("Failed to parse response JSON: {}", e))?;

        let stream_load_status =
            if let Some(status_str) = response_json.get("Status").and_then(|v| v.as_str()) {
                if status_str.to_lowercase() == "success" {
                    StreamLoadStatus::Successful
                } else {
                    StreamLoadStatus::Failure
                }
            } else {
                if response_json.get("code").is_some()
                    || response_json.get("msg").and_then(|v| v.as_str()) == Some("Unauthorized")
                {
                    StreamLoadStatus::Failure
                } else {
                    StreamLoadStatus::Failure
                }
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

        let uri = uri_str.parse::<Uri>().map_err(|e| {
            debug!(
                message = "Failed to parse health check URI.",
                %e,
                url = %uri_str
            );
            format!("Invalid health check URI: {}", e)
        })?;

        debug!(
            message = "Sending health check request to Doris FE node",
            uri = %uri
        );

        let mut request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .map_err(|e| format!("Failed to build health check request: {}", e))?;

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        let response = self.http_client.send(request).await?;
        let status = response.status();

        let (_, body) = response.into_parts();
        let body_bytes = hyper::body::to_bytes(body)
            .await
            .map_err(|e| format!("Failed to read health check response body: {}", e))?;

        if status.is_success() {
            // Parse the response JSON
            match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                Ok(json) => {
                    // Check if the msg field is "success"
                    if let Some(msg) = json.get("msg").and_then(|m| m.as_str()) {
                        if msg.to_lowercase() == "success" {
                            debug!(
                                message = "Doris FE node is healthy",
                                node = %endpoint
                            );
                            return Ok(());
                        } else {
                            debug!(
                                message = "Doris FE node returned non-success message",
                                node = %endpoint,
                                message = %msg
                            );
                            return Err(format!("Doris node health check failed: {}", msg).into());
                        }
                    }
                }
                Err(e) => {
                    return Err(format!("Failed to parse health check response: {}", e).into());
                }
            }
        }

        debug!(
            message = "Doris FE node health check failed",
            node = %endpoint,
            status = %status
        );

        Err(format!("Doris node health check failed with status: {}", status).into())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Auth as HttpAuth;
    use bytes::Bytes;
    use std::collections::HashMap;
    use vector_common::sensitive_string::SensitiveString;
    use vector_lib::config::proxy::ProxyConfig;

    #[tokio::test]
    async fn test_doris_client_send_stream_load() {
        let json_data = r#"
        {"id": 1001, "name": "Zhang San", "age": 30, "city": "Beijing", "timestamp": "2023-05-01 12:30:45"}
        {"id": 1002, "name": "Li Si", "age": 25, "city": "Shanghai", "timestamp": "2023-05-01 13:45:22"}
        {"id": 1003, "name": "Wang Wu", "age": 35, "city": "Guangzhou", "timestamp": "2023-05-01 14:20:15"}
        {"id": 1004, "name": "Zhao Liu", "age": 28, "city": "Shenzhen", "timestamp": "2023-05-01 15:10:38"}
        {"id": 1005, "name": "Qian Qi", "age": 40, "city": "Hangzhou", "timestamp": "2023-05-01 16:05:12"}
        "#.trim();

        let http_client = HttpClient::new(None, &ProxyConfig::default()).unwrap();

        let mut headers = HashMap::new();
        headers.insert("format".to_string(), "json".to_string());
        headers.insert("strip_outer_array".to_string(), "false".to_string());
        headers.insert("read_json_by_line".to_string(), "true".to_string());

        let auth = Some(HttpAuth::Basic {
            user: "root".to_string(),
            password: SensitiveString::from("123456".to_string()),
        });

        let client = DorisSinkClient::new(
            http_client,
            "http://10.16.10.6:8630".to_string(), // Update to actual Doris FE address
            auth,
            Compression::None,
            "vector_test".to_string(),
            headers,
        )
        .await;

        let database = "test_db".to_string(); // Update to your database name
        let table = "test_table".to_string(); // Update to your table name
        let payload = Bytes::from(json_data);

        println!("Sending data to Doris server...");
        println!("Database: {}, Table: {}", database, table);
        println!("JSON data sample:");
        println!(
            "{}",
            if json_data.len() > 300 {
                &json_data[..300]
            } else {
                json_data
            }
        );

        let result = client.send_stream_load(database, table, payload).await;
        match result {
            Ok(response) => {
                println!("HTTP Status Code: {}", response.http_status_code);
                println!("Stream Load Status: {:?}", response.stream_load_status);
                println!("Doris Response: {:?}", response.response_json);

                // Check key fields in the response
                if let Some(status_str) = response
                    .response_json
                    .get("status")
                    .and_then(|v| v.as_str())
                {
                    println!("Import Status: {}", status_str);
                }

                if let Some(msg) = response.response_json.get("msg").and_then(|v| v.as_str()) {
                    println!("Message: {}", msg);
                }

                if let Some(rows) = response
                    .response_json
                    .get("number_loaded_rows")
                    .and_then(|v| v.as_u64())
                {
                    println!("Imported Rows: {}", rows);
                } else if let Some(rows) = response
                    .response_json
                    .get("loaded_rows")
                    .and_then(|v| v.as_u64())
                {
                    println!("Imported Rows: {}", rows);
                }

                // Handle different statuses
                match response.stream_load_status {
                    StreamLoadStatus::Successful => {
                        println!("Data import successful!");
                    }
                    StreamLoadStatus::Failure => {
                        println!(
                            "Data import failed! Please check error message and response status."
                        );
                    }
                }
            }
            Err(e) => {
                println!("Request failed: {}", e);
            }
        }

        println!("Test completed");
    }
}
