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
use vector_lib::event::{EventFinalizers, EventStatus, Finalizable};
use vector_lib::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_lib::stream::DriverResponse;

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
    pub body: Bytes,
    pub headers: BTreeMap<String, String>,
    pub finalizers: EventFinalizers,
    pub uri: Uri,
    pub uncompressed_size: usize,
    pub metadata: RequestMetadata,
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

impl Finalizable for TraceApiRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for TraceApiRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

#[derive(Debug)]
pub struct TraceApiResponse {
    status_code: StatusCode,
    body: Bytes,
    byte_size: GroupedCountByteSize,
    uncompressed_size: usize,
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

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.uncompressed_size)
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

    // Emission of Error internal event is handled upstream by the caller
    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.client.poll_ready(cx)
    }

    // Emission of Error internal event is handled upstream by the caller
    fn call(&mut self, mut request: TraceApiRequest) -> Self::Future {
        let client = self.client.clone();

        Box::pin(async move {
            let metadata = std::mem::take(request.metadata_mut());
            let byte_size = metadata.into_events_estimated_json_encoded_byte_size();
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
                byte_size,
                uncompressed_size,
            })
        })
    }
}
