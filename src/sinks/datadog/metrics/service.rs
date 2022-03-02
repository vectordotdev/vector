use std::task::{Context, Poll};

use bytes::{Buf, Bytes};
use futures::future::BoxFuture;
use http::{
    header::{HeaderValue, CONTENT_ENCODING, CONTENT_TYPE},
    Request, StatusCode, Uri,
};
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

/// Retry logic specific to the Datadog metrics endpoints.
#[derive(Debug, Default, Clone)]
pub struct DatadogMetricsRetryLogic;

impl RetryLogic for DatadogMetricsRetryLogic {
    type Error = HttpError;
    type Response = DatadogMetricsResponse;

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

/// Generalized request for sending metrics to the Datadog metrics endpoints.
#[derive(Debug, Clone)]
pub struct DatadogMetricsRequest {
    pub payload: Bytes,
    pub uri: Uri,
    pub content_type: &'static str,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
}

impl DatadogMetricsRequest {
    /// Converts this request to a `hyper`-compatible request.
    ///
    /// # Errors
    ///
    /// If any of the header names or values are invalid, or if the URI is invalid, an error variant
    /// will be returned.
    pub fn into_http_request(self, api_key: HeaderValue) -> http::Result<Request<Body>> {
        // Requests to the metrics endpoints can be compressed, and there's almost no reason to
        // _not_ compress them given tha t metric data, when encoded, is very repetitive.  Thus,
        // here and through the sink code, we always compress requests.  Datadog also only supports
        // zlib (DEFLATE) compression, which is why it's hard-coded here vs being set via the common
        // `Compression` value that most sinks utilize.
        let request = Request::post(self.uri)
            .header("DD-API-KEY", api_key)
            // TODO: The Datadog Agent sends this header to indicate the version of the Go library
            // it uses which contains the Protocol Buffers definitions used for the Sketches API.
            // We've copypastaed the proto file for now -- `proto/ddsketch.rs`, a partial chunk of
            // `DataDog/agent-payload/proto/metrics/agent_payload.proto` -- and are thus hardcoding
            // the version that we copypasted from.
            //
            // In the future, we should likely figure out a way to depend on/submodule-ize the
            // `agent-payload` repo so we can always have an up-to-date proto definition, and be
            // able to programmatically set the version of the repo so we don't need to hardcode
            // this header.
            .header("DD-Agent-Payload", "4.87.0")
            .header(CONTENT_TYPE, self.content_type)
            .header(CONTENT_ENCODING, "deflate");

        request.body(Body::from(self.payload))
    }
}

impl Ackable for DatadogMetricsRequest {
    fn ack_size(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for DatadogMetricsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

// Generalized wrapper around the raw response from Hyper.
#[derive(Debug)]
pub struct DatadogMetricsResponse {
    status_code: StatusCode,
    body: Bytes,
    batch_size: usize,
    byte_size: usize,
}

impl DriverResponse for DatadogMetricsResponse {
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

#[derive(Clone)]
pub struct DatadogMetricsService {
    client: HttpClient,
    api_key: HeaderValue,
}

impl DatadogMetricsService {
    /// Creates a new `DatadogMetricsService`.
    pub fn new(client: HttpClient, api_key: &str) -> Self {
        DatadogMetricsService {
            client,
            api_key: HeaderValue::from_str(api_key)
                .expect("API key should be only valid ASCII characters"),
        }
    }
}

impl Service<DatadogMetricsRequest> for DatadogMetricsService {
    type Response = DatadogMetricsResponse;
    type Error = HttpError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.client.poll_ready(cx)
    }

    fn call(&mut self, request: DatadogMetricsRequest) -> Self::Future {
        let client = self.client.clone();
        let api_key = self.api_key.clone();

        Box::pin(async move {
            let byte_size = request.payload.len();
            let batch_size = request.batch_size;

            let request = request
                .into_http_request(api_key)
                .context(BuildRequestSnafu)?;
            let response = client.send(request).await?;
            let (parts, body) = response.into_parts();
            let mut body = hyper::body::aggregate(body)
                .await
                .context(CallRequestSnafu)?;
            let body = body.copy_to_bytes(body.remaining());

            Ok(DatadogMetricsResponse {
                status_code: parts.status,
                body,
                batch_size,
                byte_size,
            })
        })
    }
}
