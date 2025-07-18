use std::{
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use headers::HeaderName;
use http::{
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    HeaderValue, Request, Uri,
};
use hyper::Body;
use indexmap::IndexMap;
use tower::Service;
use tracing::Instrument;
use vector_lib::event::{EventFinalizers, EventStatus, Finalizable};
use vector_lib::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_lib::stream::DriverResponse;

use crate::{
    http::HttpClient,
    sinks::util::{retries::RetryLogic, Compression},
    sinks::{datadog::DatadogApiError, util::http::validate_headers},
};

#[derive(Debug, Default, Clone)]
pub struct LogApiRetry;

impl RetryLogic for LogApiRetry {
    type Error = DatadogApiError;
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
    user_provided_headers: IndexMap<HeaderName, HeaderValue>,
    dd_evp_headers: IndexMap<HeaderName, HeaderValue>,
}

impl LogApiService {
    pub fn new(
        client: HttpClient,
        uri: Uri,
        headers: IndexMap<String, String>,
        dd_evp_origin: String,
    ) -> crate::Result<Self> {
        let user_provided_headers = validate_headers(&headers)?;

        let dd_evp_headers = &[
            ("DD-EVP-ORIGIN".to_string(), dd_evp_origin),
            ("DD-EVP-ORIGIN-VERSION".to_string(), crate::get_version()),
        ]
        .into_iter()
        .collect();
        let dd_evp_headers = validate_headers(dd_evp_headers)?;

        Ok(Self {
            client,
            uri,
            user_provided_headers,
            dd_evp_headers,
        })
    }
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
        let mut client = self.client.clone();
        let http_request = Request::post(&self.uri)
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
            for (name, value) in &self.user_provided_headers {
                // Replace rather than append to any existing header values
                headers.insert(name, value.clone());
            }
            // Set DD EVP headers last so that they cannot be overridden.
            for (name, value) in &self.dd_evp_headers {
                headers.insert(name, value.clone());
            }
        }

        let http_request = http_request
            .body(Body::from(request.body))
            .expect("building HTTP request failed unexpectedly");

        Box::pin(async move {
            DatadogApiError::from_result(client.call(http_request).in_current_span().await).map(
                |_| LogApiResponse {
                    event_status: EventStatus::Delivered,
                    events_byte_size,
                    raw_byte_size,
                },
            )
        })
    }
}
