use crate::http::HttpClient;
use crate::sinks::util::retries::RetryLogic;
use crate::sinks::util::Compression;
use futures::future::BoxFuture;
use http::header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE};
use http::{Request, StatusCode, Uri};
use hyper::Body;
use snafu::Snafu;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::Service;
use tracing::Instrument;
use vector_core::buffers::Ackable;
use vector_core::event::{EventFinalizers, EventStatus, Finalizable};

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
pub enum LogApiResponse {
    /// Client sent a request and all was well with it.
    Ok,
    /// Client request has likely invalid API key.
    PermissionIssue,
}

impl AsRef<EventStatus> for LogApiResponse {
    fn as_ref(&self) -> &EventStatus {
        match self {
            LogApiResponse::Ok => &EventStatus::Delivered,
            LogApiResponse::PermissionIssue => &EventStatus::Errored,
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
                        StatusCode::FORBIDDEN => Ok(LogApiResponse::PermissionIssue),
                        StatusCode::OK | StatusCode::ACCEPTED => Ok(LogApiResponse::Ok),
                        StatusCode::PAYLOAD_TOO_LARGE => Err(LogApiError::PayloadTooLarge),
                        _ => Err(LogApiError::ServerError),
                    }
                }
                Err(error) => Err(LogApiError::HttpError { error }),
            }
        })
    }
}
