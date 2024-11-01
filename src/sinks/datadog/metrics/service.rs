use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::future::BoxFuture;
use http::{
    header::{HeaderValue, CONTENT_ENCODING, CONTENT_TYPE},
    Request, StatusCode, Uri,
};
use hyper::Body;
use snafu::ResultExt;
use tower::Service;
use vector_lib::event::{EventFinalizers, EventStatus, Finalizable};
use vector_lib::request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata};
use vector_lib::stream::DriverResponse;

use crate::{
    http::{BuildRequestSnafu, HttpClient},
    sinks::datadog::DatadogApiError,
    sinks::util::retries::RetryLogic,
};

/// Retry logic specific to the Datadog metrics endpoints.
#[derive(Debug, Default, Clone)]
pub struct DatadogMetricsRetryLogic;

impl RetryLogic for DatadogMetricsRetryLogic {
    type Error = DatadogApiError;
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
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
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
        // Requests to the metrics endpoints can be compressed, and there's almost no reason to
        // _not_ compress them given tha t metric data, when encoded, is very repetitive.  Thus,
        // here and through the sink code, we always compress requests.  Datadog also only supports
        // zlib (DEFLATE) compression, which is why it's hard-coded here vs being set via the common
        // `Compression` value that most sinks utilize.
        let request = Request::post(self.uri)
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
            .header(CONTENT_ENCODING, "deflate");

        request.body(Body::from(self.payload))
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

            let request = request
                .into_http_request(api_key)
                .context(BuildRequestSnafu)
                .map_err(|error| DatadogApiError::HttpError { error })?;

            let result = client.send(request).await;
            let result = DatadogApiError::from_result(result)?;

            Ok(DatadogMetricsResponse {
                status_code: result.status(),
                request_metadata,
            })
        })
    }
}
