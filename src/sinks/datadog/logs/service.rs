use std::{
    collections::BTreeMap,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use http_1::{
    Request, Uri,
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, HeaderMap},
};
use tower::Service;
use tracing::Instrument;
use vector_http::full_body;
use vector_lib::{
    event::{EventFinalizers, EventStatus, Finalizable},
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use crate::{
    http::client_v1::HttpClient,
    sinks::{
        datadog::DatadogApiError,
        util::{Compression, http::validate_headers, retries::RetryLogic},
    },
};

#[derive(Debug, Default, Clone)]
pub struct LogApiRetry;

impl RetryLogic for LogApiRetry {
    type Error = DatadogApiError;
    type Request = LogApiRequest;
    type Response = LogApiResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_retriable()
    }
}

#[derive(Debug, Clone)]
pub struct LogApiRequest {
    pub api_key: Arc<str>,
    pub compression: Compression,
    pub body: Bytes,
    pub finalizers: EventFinalizers,
    pub uncompressed_size: usize,
    pub metadata: RequestMetadata,
}

impl Finalizable for LogApiRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for LogApiRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

#[derive(Debug)]
pub struct LogApiResponse {
    event_status: EventStatus,
    events_byte_size: GroupedCountByteSize,
    raw_byte_size: usize,
}

impl DriverResponse for LogApiResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.raw_byte_size)
    }
}

/// Wrapper for the Datadog API.
///
/// Provides a `tower::Service` for the Datadog Logs API, allowing it to be
/// composed within a Tower "stack", such that we can easily and transparently
/// provide retries, concurrency limits, rate limits, and more.
#[derive(Debug, Clone)]
pub struct LogApiService {
    client: HttpClient,
    uri: Uri,
    user_provided_headers: HeaderMap,
    dd_evp_headers: HeaderMap,
}

impl LogApiService {
    /// Creates a logs API service with validated user and Datadog origin headers.
    ///
    /// # Errors
    ///
    /// Returns an error if user-provided or Datadog origin headers are invalid.
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::needless_pass_by_value,
        reason = "Preserve the constructor's public signature and header ownership."
    )]
    pub fn new(
        client: HttpClient,
        uri: Uri,
        headers: BTreeMap<String, String>,
        dd_evp_origin: String,
    ) -> crate::Result<Self> {
        let user_provided_headers = validated_header_map(&headers)?;

        let dd_evp_headers: BTreeMap<String, String> = [
            ("DD-EVP-ORIGIN".to_string(), dd_evp_origin),
            ("DD-EVP-ORIGIN-VERSION".to_string(), crate::get_version()),
        ]
        .into_iter()
        .collect();
        let dd_evp_headers = validated_header_map(&dd_evp_headers)?;

        Ok(Self {
            client,
            uri,
            user_provided_headers,
            dd_evp_headers,
        })
    }
}

/// Validates header names and values and collects them into a header map.
///
/// Entries are inserted in the sorted order of `validate_headers`, so the
/// resulting map preserves the same header order as the legacy implementation.
///
/// # Errors
///
/// Returns an error if a header name or value is invalid.
fn validated_header_map(headers: &BTreeMap<String, String>) -> crate::Result<HeaderMap> {
    let validated = validate_headers(headers)?;
    let mut map = HeaderMap::with_capacity(validated.len());
    for (name, value) in validated {
        map.insert(name.inner().clone(), value);
    }
    Ok(map)
}

impl Service<LogApiRequest> for LogApiService {
    type Response = LogApiResponse;
    type Error = DatadogApiError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of Error internal event is handled upstream by the caller
    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of Error internal event is handled upstream by the caller
    fn call(&mut self, mut request: LogApiRequest) -> Self::Future {
        let client = self.client.clone();
        let http_request = Request::post(self.uri.clone())
            .header(CONTENT_TYPE, "application/json")
            .header("DD-API-KEY", request.api_key.to_string());

        let http_request = if let Some(ce) = request.compression.content_encoding() {
            http_request.header(CONTENT_ENCODING, ce)
        } else {
            http_request
        };

        let metadata = std::mem::take(request.metadata_mut());
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();
        let raw_byte_size = request.uncompressed_size;

        let mut http_request = http_request.header(CONTENT_LENGTH, request.body.len());

        if let Some(headers) = http_request.headers_mut() {
            // Replace rather than append to any existing header values
            for (name, value) in &self.user_provided_headers {
                headers.insert(name, value.clone());
            }
            // Set DD EVP headers last so that they cannot be overridden.
            for (name, value) in &self.dd_evp_headers {
                headers.insert(name, value.clone());
            }
        }

        let http_request = http_request
            .body(request.body)
            .expect("building HTTP request failed unexpectedly");

        Box::pin(async move {
            DatadogApiError::from_result(
                client
                    .send(http_request.map(full_body))
                    .in_current_span()
                    .await,
            )
            .map(|_| LogApiResponse {
                event_status: EventStatus::Delivered,
                events_byte_size,
                raw_byte_size,
            })
        })
    }
}
