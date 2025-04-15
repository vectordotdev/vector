use bytes::Bytes;
use futures::future::BoxFuture;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use http::{Method, Response, StatusCode, Uri};
use hyper::{service::Service, Body, Request};
use std::time::SystemTime;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    task::{Context, Poll},
};
use tower::ServiceExt;
use tracing::{debug, info};
use uuid::Uuid;
use vector_lib::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_lib::stream::DriverResponse;
use vector_lib::ByteSizeOf;

use super::DorisConfig;
use crate::sinks::doris::common::DorisCommon;
use crate::sinks::doris::sink::DorisPartitionKey;
use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    http::HttpClient,
    sinks::util::{
        auth::Auth,
        http::{HttpBatchService, RequestConfig},
        Compression, ElementCount,
    },
};

#[derive(Clone, Debug)]
pub struct DorisRequest {
    pub payload: Bytes,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
    pub partition_key: DorisPartitionKey,
    pub redirect_url: Option<String>,
}

impl ByteSizeOf for DorisRequest {
    fn allocated_bytes(&self) -> usize {
        self.payload.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

impl ElementCount for DorisRequest {
    fn element_count(&self) -> usize {
        self.metadata.event_count()
    }
}

impl Finalizable for DorisRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for DorisRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

pub struct HttpRequestBuilder {
    pub auth: Option<Auth>,
    pub compression: Compression,
    pub http_request_config: RequestConfig,
    pub base_url: String,
    pub label_prefix: String,
    pub headers: HashMap<String, String>,
}

impl HttpRequestBuilder {
    pub fn new(common: &DorisCommon, config: &DorisConfig) -> HttpRequestBuilder {
        let auth = common.auth.clone().map(|http_auth| Auth::Basic(http_auth));

        // Create and set headers
        let mut headers = HashMap::new();
        // Basic headers
        headers.insert("Expect".to_string(), "100-continue".to_string());
        headers.insert(
            "Content-Type".to_string(),
            "text/plain;charset=utf-8".to_string(),
        );

        // Add line delimiter header (if non-default)
        if !config.line_delimiter.is_empty() && config.line_delimiter != "\n" {
            headers.insert("line_delimiter".to_string(), config.line_delimiter.clone());
        }

        // Add custom headers
        for (k, v) in &config.headers {
            headers.insert(k.clone(), v.clone());
        }

        HttpRequestBuilder {
            auth,
            compression: config.compression.clone(),
            http_request_config: config.request.clone(),
            base_url: common.base_url.clone(),
            label_prefix: config.label_prefix.clone(),
            // http_client: None,
            headers,
        }
    }

    pub async fn build_request(
        &self,
        doris_req: DorisRequest,
    ) -> Result<Request<Bytes>, crate::Error> {
        let database = &doris_req.partition_key.database;
        let table = &doris_req.partition_key.table;

        debug!(
            message = "Building Doris Stream Load request",
            database = %database,
            table = %table,
            payload_size = doris_req.payload.len()
        );

        // Generate a unique label
        let label = format!(
            "{}_{}_{}_{}_{}",
            self.label_prefix,
            database,
            table,
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            Uuid::new_v4()
        );

        // Check if there is a redirect URL
        let uri = if let Some(redirect_url) = &doris_req.redirect_url {
            // Use redirect URL
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
            .header(CONTENT_LENGTH, doris_req.payload.len())
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

        // First add headers we created in the constructor
        for (header, value) in &self.headers {
            builder = builder.header(&header[..], &value[..]);
        }

        // Add custom headers from http_request_config (for compatibility)
        for (header, value) in &self.http_request_config.headers {
            builder = builder.header(&header[..], &value[..]);
        }

        let mut request = builder.body(doris_req.payload).map_err(|error| {
            debug!(
                message = "Failed to build HTTP request.",
                %error,
                uri = %uri
            );
            crate::Error::from(format!("Failed to build request: {}", error))
        })?;

        // Apply authentication if configured
        if let Some(auth) = &self.auth {
            if let Auth::Basic(http_auth) = auth {
                http_auth.apply(&mut request);
                debug!(
                    message = "Applied Basic authentication to request.",
                    uri = %request.uri()
                );
            }
        }

        debug!(
            message = "Request built successfully",
            method = %request.method(),
            uri = %request.uri(),
            headers_count = request.headers().len()
        );

        Ok(request)
    }
}

pub struct DorisResponse {
    pub http_response: Response<Bytes>,
    pub event_status: EventStatus,
    pub events_byte_size: GroupedCountByteSize,
}

impl DriverResponse for DorisResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }
}

#[derive(Clone)]
pub struct DorisService {
    batch_service: HttpBatchService<
        BoxFuture<'static, Result<http::Request<Bytes>, crate::Error>>,
        DorisRequest,
    >,
    log_request: bool,
    reporter: Arc<super::progress::ProgressReporter>,
}

impl DorisService {
    pub fn new(
        http_client: HttpClient<Body>,
        http_request_builder: HttpRequestBuilder,
        log_request: bool,
        reporter: Arc<super::progress::ProgressReporter>,
    ) -> DorisService {
        let http_request_builder = Arc::new(http_request_builder);
        let batch_service = HttpBatchService::new(http_client, move |req| {
            let request_builder = Arc::clone(&http_request_builder);
            let future: BoxFuture<'static, Result<http::Request<Bytes>, crate::Error>> =
                Box::pin(async move { request_builder.build_request(req).await });
            future
        });

        DorisService {
            batch_service: batch_service,
            log_request: log_request,
            reporter: reporter,
        }
    }
}

impl Service<DorisRequest> for DorisService {
    type Response = DorisResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, mut req: DorisRequest) -> Self::Future {
        // Clone necessary data for use in async closure
        let mut http_service = self.batch_service.clone();
        let log_request = self.log_request;
        let reporter = Arc::clone(&self.reporter);

        Box::pin(async move {
            // Ensure service is ready
            http_service.ready().await?;

            // Process metadata and byte size calculation
            let events_byte_size =
                std::mem::take(req.metadata_mut()).into_events_estimated_json_encoded_byte_size();

            // Create the current request and track visited URLs to prevent redirect loops
            let mut current_req = req;
            let mut redirect_count = 0;
            let mut visited_urls = HashSet::new();
            const MAX_REDIRECTS: u8 = 3;

            // Record initial URL
            if let Some(url) = &current_req.redirect_url {
                visited_urls.insert(url.clone());
            }

            // Send initial request
            let mut http_response = http_service.call(current_req.clone()).await?;

            // Check if received a redirect response
            let mut status = http_response.status();

            // Handle redirects
            while (status == StatusCode::TEMPORARY_REDIRECT
                || status == StatusCode::PERMANENT_REDIRECT
                || status == StatusCode::FOUND)
                && redirect_count < MAX_REDIRECTS
            {
                // Try to get redirect location
                if let Some(location) = http_response.headers().get(http::header::LOCATION) {
                    if let Ok(location_str) = location.to_str() {
                        debug!(
                            message = "Following redirect",
                            status = %status,
                            to = %location_str,
                            redirect_count = redirect_count + 1
                        );

                        // Check for redirect loops
                        if !visited_urls.insert(location_str.to_string()) {
                            return Err(crate::Error::from("Detected redirect loop"));
                        }

                        // Create a new DorisRequest using the redirect URL
                        let redirect_req = DorisRequest {
                            redirect_url: Some(location_str.to_string()),
                            ..current_req.clone()
                        };

                        // Send redirect request
                        http_service.ready().await?;
                        http_response = http_service.call(redirect_req.clone()).await?;
                        status = http_response.status();

                        // Update current request to redirect request
                        current_req = redirect_req;

                        // Increment redirect count
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

            // Check if exceeded maximum redirects
            if redirect_count >= MAX_REDIRECTS {
                return Err(crate::Error::from(format!(
                    "Exceeded maximum number of redirects ({})",
                    MAX_REDIRECTS
                )));
            }

            // Process final response
            let event_status = get_event_status(&http_response);

            // Log final response body - regardless of success or failure
            if log_request {
                let body = http_response.body();
                let body_str = String::from_utf8_lossy(body);
                info!(
                    message = "Doris stream load response received.",
                    status_code = %status,
                    response = %body_str
                );

                // If response is successful, try to parse response body and update progress statistics
                if status.is_success() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_str) {
                        if let Some(status_value) = json.get("Status") {
                            if let Some(status_str) = status_value.as_str() {
                                if status_str == "Success" {
                                    // Update byte count statistics
                                    if let Some(load_bytes) =
                                        json.get("LoadBytes").and_then(|b| b.as_i64())
                                    {
                                        reporter.incr_total_bytes(load_bytes);
                                        debug!(
                                            message = "Updated bytes statistics.",
                                            added_bytes = load_bytes
                                        );
                                    }

                                    // Update row count statistics
                                    if let Some(loaded_rows) =
                                        json.get("NumberLoadedRows").and_then(|r| r.as_i64())
                                    {
                                        reporter.incr_total_rows(loaded_rows);
                                        debug!(
                                            message = "Updated rows statistics.",
                                            added_rows = loaded_rows
                                        );
                                    }

                                    // Update filtered rows count statistics
                                    if let Some(filtered_rows) =
                                        json.get("NumberFilteredRows").and_then(|r| r.as_i64())
                                    {
                                        if filtered_rows > 0 {
                                            reporter.incr_failed_rows(filtered_rows);
                                            debug!(
                                                message = "Updated filtered rows statistics.",
                                                filtered_rows = %filtered_rows
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(DorisResponse {
                event_status,
                http_response,
                events_byte_size,
            })
        })
    }
}

fn get_event_status(response: &Response<Bytes>) -> EventStatus {
    let status = response.status();

    // Handle basic HTTP status level
    if !status.is_success() {
        if status.is_server_error() {
            // Server errors are typically temporary
            debug!(
                message = "Detected server error status code.",
                status_code = %status
            );
            return EventStatus::Errored;
        } else {
            // Client errors and other non-success statuses are considered permanent failures
            debug!(
                message = "Detected client error or other non-success status code.",
                status_code = %status
            );
            return EventStatus::Rejected;
        }
    }

    // HTTP status code is 2xx, need to further parse the response body
    let body = String::from_utf8_lossy(response.body());

    // Try to parse response body as JSON
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(json) => {
            // Check Doris-specific Status field
            if let Some(status_value) = json.get("Status") {
                if let Some(status_str) = status_value.as_str() {
                    if status_str == "Success" {
                        // Clearly successful case
                        debug!(
                            message = "Received successful status from Doris.",
                            doris_status = %status_str
                        );
                        return EventStatus::Delivered;
                    } else {
                        // Non-success status is considered rejected
                        debug!(
                            message = "Received non-success status from Doris.",
                            doris_status = %status_str
                        );
                        return EventStatus::Rejected;
                    }
                }
            }

            // Fallback - if Status field not found but format is JSON
            if body.contains("\"errors\":true") || body.contains("\"status\":\"Fail\"") {
                debug!(
                    message = "Detected error indicators in JSON response body.",
                    contains_errors = body.contains("\"errors\":true"),
                    contains_fail = body.contains("\"status\":\"Fail\"")
                );
                return EventStatus::Rejected;
            }

            // No clear error indicators, assume success
            debug!(message = "No error indicators found in JSON response, assuming success.");
            return EventStatus::Delivered;
        }
        Err(error) => {
            // Cannot parse JSON, try to determine based on text content
            debug!(
                message = "Failed to parse response as JSON, falling back to text analysis.",
                %error
            );
            if body.contains("Success") {
                debug!(message = "Detected 'Success' in plain text response.");
                return EventStatus::Delivered;
            } else {
                debug!(message = "No success indicators found in plain text response.");
                return EventStatus::Rejected;
            }
        }
    }
}
