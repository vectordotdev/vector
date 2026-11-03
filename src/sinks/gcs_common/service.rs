use std::task::Poll;

use bytes::Bytes;
use futures::future::BoxFuture;
use http::{
    Request,
    header::{HeaderName, HeaderValue},
};
use hyper_1::body::Incoming;
use tower::Service;
use vector_http::{HttpError, full_body};
use vector_lib::{
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    gcp::GcpAuthenticator,
    http::client_v1::HttpClient,
    sinks::util::HttpEndpoint,
};

#[derive(Debug, Clone)]
pub struct GcsService {
    client: HttpClient,
    base_url: HttpEndpoint,
    auth: GcpAuthenticator,
}

impl GcsService {
    pub const fn new(
        client: HttpClient,
        base_url: HttpEndpoint,
        auth: GcpAuthenticator,
    ) -> GcsService {
        GcsService {
            client,
            base_url,
            auth,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GcsRequest {
    pub key: String,
    pub body: Bytes,
    pub settings: GcsRequestSettings,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
}

impl Finalizable for GcsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for GcsRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

// Settings required to produce a request that do not change per
// request. All possible values are pre-computed for direct use in
// producing a request.
#[derive(Clone, Debug)]
pub struct GcsRequestSettings {
    pub acl: Option<HeaderValue>,
    pub content_type: HeaderValue,
    pub content_encoding: Option<HeaderValue>,
    pub storage_class: HeaderValue,
    pub cache_control: Option<HeaderValue>,
    pub headers: Vec<(HeaderName, HeaderValue)>,
}

/// Response of a GCS-family request, shared by the `gcp_cloud_storage` and `gcp_chronicle`
/// services so both use [`GcsRetryLogic`](super::config::GcsRetryLogic).
#[derive(Debug)]
pub struct GcsResponse {
    /// The native client's response; delivery and retries use only its status, so the body
    /// stays unread.
    pub inner: http::Response<Incoming>,
    pub metadata: RequestMetadata,
}

impl DriverResponse for GcsResponse {
    fn event_status(&self) -> EventStatus {
        if self.inner.status().is_success() {
            EventStatus::Delivered
        } else if self.inner.status().is_server_error() {
            EventStatus::Errored
        } else {
            EventStatus::Rejected
        }
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        self.metadata.events_estimated_json_encoded_byte_size()
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.metadata.request_encoded_size())
    }
}

impl Service<GcsRequest> for GcsService {
    type Response = GcsResponse;
    type Error = HttpError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, request: GcsRequest) -> Self::Future {
        let settings = request.settings;
        let metadata = request.metadata;

        let uri = self
            .base_url
            .append_path(&request.key)
            .expect("partition key must be a valid URI path segment")
            .into_uri();

        let mut builder = Request::put(uri);
        let headers = builder.headers_mut().unwrap();
        headers.insert("content-type", settings.content_type);
        headers.insert(
            "content-length",
            HeaderValue::from_str(&request.body.len().to_string()).unwrap(),
        );
        settings
            .content_encoding
            .map(|ce| headers.insert("content-encoding", ce));
        settings.acl.map(|acl| headers.insert("x-goog-acl", acl));
        settings
            .cache_control
            .map(|cc| headers.insert("cache-control", cc));
        headers.insert("x-goog-storage-class", settings.storage_class);
        for (p, v) in settings.headers {
            headers.insert(p, v);
        }

        let mut http_request = builder.body(request.body).unwrap();
        self.auth.apply(&mut http_request);

        let client = self.client.clone();
        Box::pin(async move {
            let result = client.send(http_request.map(full_body)).await;
            result.map(|inner| GcsResponse { inner, metadata })
        })
    }
}
