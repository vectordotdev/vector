use std::{
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use http::{
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    Request, Uri,
};
use hyper::Body;
use tower::Service;
use tracing::Instrument;
use vector_core::{
    event::{EventFinalizers, EventStatus, Finalizable},
    internal_event::CountByteSize,
    stream::DriverResponse,
};

use crate::{
    http::HttpClient,
    sinks::datadog::DatadogApiError,
    sinks::util::{retries::RetryLogic, Compression},
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
    pub batch_size: usize,
    pub api_key: Arc<str>,
    pub compression: Compression,
    pub body: Bytes,
    pub finalizers: EventFinalizers,
    pub events_byte_size: usize,
    pub uncompressed_size: usize,
}

impl Finalizable for LogApiRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

#[derive(Debug)]
pub struct LogApiResponse {
    event_status: EventStatus,
    count: usize,
    events_byte_size: usize,
    raw_byte_size: usize,
    protocol: String,
}

impl DriverResponse for LogApiResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(self.count, self.events_byte_size)
    }

    fn bytes_sent(&self) -> Option<(usize, &str)> {
        Some((self.raw_byte_size, &self.protocol))
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
    enterprise: bool,
}

impl LogApiService {
    pub const fn new(client: HttpClient, uri: Uri, enterprise: bool) -> Self {
        Self {
            client,
            uri,
            enterprise,
        }
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
    fn call(&mut self, request: LogApiRequest) -> Self::Future {
        let mut client = self.client.clone();
        let http_request = Request::post(&self.uri)
            .header(CONTENT_TYPE, "application/json")
            .header(
                "DD-EVP-ORIGIN",
                if self.enterprise {
                    "vector-enterprise"
                } else {
                    "vector"
                },
            )
            .header("DD-EVP-ORIGIN-VERSION", crate::get_version())
            .header("DD-API-KEY", request.api_key.to_string());

        let http_request = if let Some(ce) = request.compression.content_encoding() {
            http_request.header(CONTENT_ENCODING, ce)
        } else {
            http_request
        };

        let count = request.batch_size;
        let events_byte_size = request.events_byte_size;
        let raw_byte_size = request.uncompressed_size;
        let protocol = self.uri.scheme_str().unwrap_or("http").to_string();

        let http_request = http_request
            .header(CONTENT_LENGTH, request.body.len())
            .body(Body::from(request.body))
            .expect("building HTTP request failed unexpectedly");

        Box::pin(async move {
            DatadogApiError::from_result(client.call(http_request).in_current_span().await).map(
                |_| LogApiResponse {
                    event_status: EventStatus::Delivered,
                    count,
                    events_byte_size,
                    raw_byte_size,
                    protocol,
                },
            )
        })
    }
}
