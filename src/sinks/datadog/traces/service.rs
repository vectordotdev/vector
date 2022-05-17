use std::{
    collections::BTreeMap,
    task::{Context, Poll},
};

use bytes::{Buf, Bytes};
use futures::future::BoxFuture;
use http::{Request, StatusCode, Uri};
use hyper::Body;
use snafu::ResultExt;
use tower::Service;
use vector_common::internal_event::BytesSent;
use vector_core::{
    buffers::Ackable,
    event::{EventFinalizers, EventStatus, Finalizable},
    internal_event::EventsSent,
    stream::DriverResponse,
};

use crate::{
    http::{BuildRequestSnafu, CallRequestSnafu, HttpClient, HttpError},
    sinks::util::retries::{RetryAction, RetryLogic},
};

#[derive(Debug, Default, Clone)]
pub struct TraceApiRetry;

impl RetryLogic for TraceApiRetry {
    type Error = HttpError;
    type Response = TraceApiResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.status_code;
        match status {
            // Use the same status code/retry policy as the Trace agent, additionally retrying
            // forbidden requests.
            //
            // This retry logic will be expanded further, but specifically retrying unauthorized
            // requests for now. I verified using `curl` that `403` is the respose code for this.
            //
            // https://github.com/vectordotdev/vector/issues/10870
            // https://github.com/vectordotdev/vector/issues/12220
            StatusCode::FORBIDDEN => RetryAction::Retry("forbidden".into()),
            StatusCode::REQUEST_TIMEOUT => RetryAction::Retry("request timeout".into()),
            _ if status.is_server_error() => RetryAction::Retry(
                format!("{}: {}", status, String::from_utf8_lossy(&response.body)).into(),
            ),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {}", status).into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TraceApiRequest {
    pub batch_size: usize,
    pub body: Bytes,
    pub headers: BTreeMap<String, String>,
    pub finalizers: EventFinalizers,
    pub uri: Uri,
    pub uncompressed_size: usize,
}

impl TraceApiRequest {
    pub fn into_http_request(self) -> http::Result<Request<Body>> {
        let mut request = Request::post(self.uri);
        for (k, v) in self.headers.iter() {
            request = request.header(k, v);
        }
        request.body(Body::from(self.body))
    }
}

impl Ackable for TraceApiRequest {
    fn ack_size(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for TraceApiRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

#[derive(Debug)]
pub struct TraceApiResponse {
    status_code: StatusCode,
    body: Bytes,
    batch_size: usize,
    byte_size: usize,
    uncompressed_size: usize,
    protocol: String,
}

impl DriverResponse for TraceApiResponse {
    fn event_status(&self) -> EventStatus {
        if self.status_code.is_success() {
            EventStatus::Delivered
        } else if self.status_code.is_client_error() {
            EventStatus::Rejected
        } else {
            EventStatus::Errored
        }
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.batch_size,
            byte_size: self.byte_size,
            output: None,
        }
    }

    fn bytes_sent(&self) -> Option<BytesSent> {
        Some(BytesSent {
            byte_size: self.uncompressed_size,
            protocol: &self.protocol,
        })
    }
}

/// Wrapper for the Datadog API.
///
/// Provides a `tower::Service` for the Datadog Traces API, allowing it to be
/// composed within a Tower "stack", such that we can easily and transparently
/// provide retries, concurrency limits, rate limits, and more.
#[derive(Debug, Clone)]
pub struct TraceApiService {
    client: HttpClient,
}

impl TraceApiService {
    pub const fn new(client: HttpClient) -> Self {
        Self { client }
    }
}

impl Service<TraceApiRequest> for TraceApiService {
    type Response = TraceApiResponse;
    type Error = HttpError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.client.poll_ready(cx)
    }

    fn call(&mut self, request: TraceApiRequest) -> Self::Future {
        let client = self.client.clone();
        let protocol = request.uri.scheme_str().unwrap_or("http").to_string();

        Box::pin(async move {
            let byte_size = request.body.len();
            let batch_size = request.batch_size;
            let uncompressed_size = request.uncompressed_size;
            let http_request = request.into_http_request().context(BuildRequestSnafu)?;

            let response = client.send(http_request).await?;
            let (parts, body) = response.into_parts();
            let mut body = hyper::body::aggregate(body)
                .await
                .context(CallRequestSnafu)?;
            let body = body.copy_to_bytes(body.remaining());

            Ok(TraceApiResponse {
                status_code: parts.status,
                body,
                batch_size,
                byte_size,
                protocol,
                uncompressed_size,
            })
        })
    }
}
