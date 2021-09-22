use crate::http::HttpClient;
use crate::sinks::util::retries::RetryLogic;
use futures::future::BoxFuture;
use http::{Request, StatusCode, Uri};
use hyper::Body;
use snafu::Snafu;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::Service;
use tracing::Instrument;
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
    pub(crate) serialized_payload_bytes_len: usize,
    pub(crate) payload_members_len: usize,
    pub(crate) api_key: Arc<str>,
    pub(crate) is_compressed: bool,
    pub(crate) body: Vec<u8>,
    pub(crate) finalizers: EventFinalizers,
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
}

impl LogApiService {
    pub const fn new(client: HttpClient, uri: Uri) -> Self {
        Self { client, uri }
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
            .header("Content-Type", "application/json")
            .header("DD-API-KEY", request.api_key.to_string());
        let http_request = if request.is_compressed {
            http_request.header("Content-Encoding", "gzip")
        } else {
            http_request
        };
        let http_request = http_request
            .header("Content-Length", request.body.len())
            .body(Body::from(request.body))
            .expect("TODO");

        Box::pin(async move {
            match client.call(http_request).in_current_span().await {
                Ok(response) => {
                    let status = response.status();
                    // From https://docs.datadoghq.com/api/latest/logs/:
                    //
                    // The status codes answered by the HTTP API are:
                    // 200: OK
                    // 400: Bad request (likely an issue in the payload
                    //      formatting)
                    // 403: Permission issue (likely using an invalid API Key)
                    // 413: Payload too large (batch is above 5MB uncompressed)
                    // 5xx: Internal error, request should be retried after some
                    //      time
                    match status {
                        StatusCode::BAD_REQUEST => Err(LogApiError::BadRequest),
                        StatusCode::FORBIDDEN => Ok(LogApiResponse::PermissionIssue),
                        StatusCode::OK => Ok(LogApiResponse::Ok),
                        StatusCode::PAYLOAD_TOO_LARGE => Err(LogApiError::PayloadTooLarge),
                        _ => Err(LogApiError::ServerError),
                    }
                }
                Err(error) => Err(LogApiError::HttpError { error }),
            }
        })
    }
}
