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
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
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
        Box::pin(async move {
            let byte_size = request.body.len();
            let batch_size = request.batch_size;
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
            })
        })
    }
}
