use std::{
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use http::{
    Request, StatusCode, Uri,
    header::{CONTENT_ENCODING, CONTENT_TYPE, HeaderValue},
};
use hyper::Body;
use snafu::ResultExt;
use tower::Service;
use vector_lib::{
    event::{EventFinalizers, EventStatus, Finalizable},
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use crate::{
    http::{BuildRequestSnafu, HttpClient},
    internal_events::DatadogMetricsRequestError,
    sinks::{datadog::DatadogApiError, util::retries::RetryLogic},
};

/// Retry logic specific to the Datadog metrics endpoints.
#[derive(Debug, Default, Clone)]
pub struct DatadogMetricsRetryLogic;

impl RetryLogic for DatadogMetricsRetryLogic {
    type Error = DatadogApiError;
    type Request = DatadogMetricsRequest;
    type Response = DatadogMetricsResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_retriable()
    }
}

/// Generalized request for sending metrics to the Datadog metrics endpoints.
#[derive(Debug, Clone)]
pub struct DatadogMetricsRequest {
    pub api_key: Option<Arc<str>>,
    pub payload: Bytes,
    pub uri: Uri,
    pub content_type: &'static str,
    pub content_encoding: &'static str,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
    /// Shared transaction ID linking a V2 and V3 shadow payload from the same flush.
    /// When set, `X-Metrics-Request-ID/Seq/Len` headers are included on the request.
    pub batch_id: Option<Arc<str>>,
    /// 0-based index of this request within the current flush (for split payloads).
    pub batch_seq: usize,
    /// Total number of requests produced by the current flush (for split payloads).
    pub batch_len: usize,
}

impl DatadogMetricsRequest {
    /// Converts this request to a `hyper`-compatible request.
    ///
    /// # Errors
    ///
    /// If any of the header names or values are invalid, or if the URI is invalid, an error variant
    /// will be returned.
    pub fn into_http_request(self, api_key: HeaderValue) -> http::Result<Request<Body>> {
        // use the API key from the incoming event if it is provided
        let api_key = self.api_key.map_or_else(
            || api_key,
            |key| {
                HeaderValue::from_str(&key).expect("API key should be only valid ASCII characters")
            },
        );

        let mut builder = Request::post(self.uri)
            .header("DD-API-KEY", api_key)
            // TODO: The Datadog Agent sends this header to indicate the version of the Go library
            // it uses which contains the Protocol Buffers definitions used for the Sketches API.
            // We've copypasted the proto file for now -- `proto/ddsketch.rs`, a partial chunk of
            // `DataDog/agent-payload/proto/metrics/agent_payload.proto` -- and are thus hardcoding
            // the version that we copypasted from.
            //
            // In the future, we should likely figure out a way to depend on/submodule-ize the
            // `agent-payload` repo so we can always have an up-to-date proto definition, and be
            // able to programmatically set the version of the repo so we don't need to hardcode
            // this header.
            .header("DD-Agent-Payload", "4.87.0")
            .header(CONTENT_TYPE, self.content_type)
            .header(CONTENT_ENCODING, self.content_encoding);

        if let Some(id) = &self.batch_id {
            builder = builder
                .header("X-Metrics-Request-ID", id.as_ref())
                .header("X-Metrics-Request-Seq", self.batch_seq.to_string())
                .header("X-Metrics-Request-Len", self.batch_len.to_string());
        }

        builder.body(Body::from(self.payload))
    }
}

impl Finalizable for DatadogMetricsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for DatadogMetricsRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

// Generalized wrapper around the raw response from Hyper.
#[derive(Debug)]
pub struct DatadogMetricsResponse {
    status_code: StatusCode,
    request_metadata: RequestMetadata,
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

    fn events_sent(&self) -> &GroupedCountByteSize {
        self.request_metadata
            .events_estimated_json_encoded_byte_size()
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.request_metadata.request_encoded_size())
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
    type Error = DatadogApiError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of Error internal event is handled upstream by the caller
    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.client
            .poll_ready(cx)
            .map_err(|error| DatadogApiError::HttpError { error })
    }

    // Emission of Error internal event is handled upstream by the caller
    fn call(&mut self, mut request: DatadogMetricsRequest) -> Self::Future {
        let client = self.client.clone();
        let api_key = self.api_key.clone();

        Box::pin(async move {
            let request_metadata = std::mem::take(request.metadata_mut());
            let batch_id = request.batch_id.clone();
            let uri = request.uri.clone();
            let batch_seq = request.batch_seq;
            let batch_len = request.batch_len;
            let start = std::time::Instant::now();

            let call_result: Result<_, DatadogApiError> = async {
                let http_request = request
                    .into_http_request(api_key)
                    .context(BuildRequestSnafu)
                    .map_err(|error| DatadogApiError::HttpError { error })?;

                let result = client.send(http_request).await;
                DatadogApiError::from_result(result)
            }
            .await;

            let result = call_result.inspect_err(|error| {
                emit!(DatadogMetricsRequestError {
                    error: &error.to_string(),
                    batch_id: batch_id.as_deref(),
                    uri: &uri,
                });
            })?;

            // Only batch_id-tagged requests are logged on success (dual-write shadow flushes,
            // which are rare — sampled once per `shadow_every`), so this stays low-volume and
            // gives visibility into dispatch timing for both the V2 and V3 twins of a flush.
            if let Some(id) = batch_id.as_deref() {
                info!(
                    message = "Sent Datadog metrics request.",
                    batch_id = id,
                    %uri,
                    batch_seq,
                    batch_len,
                    status = %result.status(),
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    internal_log_rate_limit = false,
                );
            }

            Ok(DatadogMetricsResponse {
                status_code: result.status(),
                request_metadata,
            })
        })
    }
}
