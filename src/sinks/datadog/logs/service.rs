use std::{
    sync::Arc,
    task::{Context, Poll},
};

use futures::future::BoxFuture;
use http::{
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    Request, StatusCode, Uri,
};
use hyper::Body;
use snafu::Snafu;
use tower::Service;
use tracing::Instrument;
use vector_core::{
    buffers::Ackable,
    event::{EventFinalizers, EventStatus, Finalizable},
    internal_event::EventsSent,
    stream::DriverResponse,
};

use crate::{
    http::HttpClient,
    sinks::util::{retries::RetryLogic, Compression},
};

#[derive(Debug, Default, Clone)]
pub struct LogApiRetry;

impl RetryLogic for LogApiRetry {
    type Error = LogApiError;
    type Response = LogApiResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match *error {
            LogApiError::HttpError { .. }
            | LogApiError::BadRequest
            | LogApiError::PayloadTooLarge => false,
            LogApiError::ServerError => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogApiRequest {
    pub batch_size: usize,
    pub api_key: Arc<str>,
    pub compression: Compression,
    pub body: Vec<u8>,
    pub finalizers: EventFinalizers,
    pub events_byte_size: usize,
}

impl Ackable for LogApiRequest {
    fn ack_size(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for LogApiRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

#[derive(Debug, Snafu)]
pub enum LogApiError {
    #[snafu(display("Server responded with an error."))]
    ServerError,
    #[snafu(display("Failed to make HTTP(S) request: {}", error))]
    HttpError { error: crate::http::HttpError },
    #[snafu(display("Client sent a payload that is too large."))]
    PayloadTooLarge,
    #[snafu(display("Client request was not valid for unknown reasons."))]
    BadRequest,
}

#[derive(Debug)]
pub struct LogApiResponse {
    event_status: EventStatus,
    count: usize,
    events_byte_size: usize,
}

impl DriverResponse for LogApiResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.count,
            byte_size: self.events_byte_size,
            output: None,
        }
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
    type Error = LogApiError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

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

        let http_request = http_request
            .header(CONTENT_LENGTH, request.body.len())
            .body(Body::from(request.body))
            .expect("building HTTP request failed unexpectedly");

        let count = request.batch_size;
        let events_byte_size = request.events_byte_size;
        Box::pin(async move {
            match client.call(http_request).in_current_span().await {
                Ok(response) => {
                    let status = response.status();
                    // From https://docs.datadoghq.com/api/latest/logs/:
                    //
                    // The status codes answered by the HTTP API are:
                    // 200: OK (v1)
                    // 202: Accepted (v2)
                    // 400: Bad request (likely an issue in the payload
                    //      formatting)
                    // 403: Permission issue (likely using an invalid API Key)
                    // 413: Payload too large (batch is above 5MB uncompressed)
                    // 5xx: Internal error, request should be retried after some
                    //      time
                    match status {
                        StatusCode::BAD_REQUEST => Err(LogApiError::BadRequest),
                        StatusCode::FORBIDDEN => Ok(LogApiResponse {
                            event_status: EventStatus::Errored,
                            count,
                            events_byte_size,
                        }),
                        StatusCode::OK | StatusCode::ACCEPTED => Ok(LogApiResponse {
                            event_status: EventStatus::Delivered,
                            count,
                            events_byte_size,
                        }),
                        StatusCode::PAYLOAD_TOO_LARGE => Err(LogApiError::PayloadTooLarge),
                        _ => Err(LogApiError::ServerError),
                    }
                }
                Err(error) => Err(LogApiError::HttpError { error }),
            }
        })
    }
}
