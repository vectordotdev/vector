use std::task::Poll;

use bytes::Bytes;
use futures::future::BoxFuture;
use http::{
    header::{HeaderName, HeaderValue},
    Request, Uri,
};
use hyper::Body;
use tower::Service;
use vector_common::internal_event::BytesSent;
use vector_core::{buffers::Ackable, internal_event::EventsSent, stream::DriverResponse};

use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    gcp::GcpCredentials,
    http::{get_http_scheme_from_uri, HttpClient, HttpError},
    sinks::util::metadata::RequestMetadata,
};

#[derive(Debug, Clone)]
pub struct GcsService {
    client: HttpClient,
    base_url: String,
    creds: Option<GcpCredentials>,
}

impl GcsService {
    pub const fn new(
        client: HttpClient,
        base_url: String,
        creds: Option<GcpCredentials>,
    ) -> GcsService {
        GcsService {
            client,
            base_url,
            creds,
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

impl Ackable for GcsRequest {
    fn ack_size(&self) -> usize {
        self.metadata.event_count()
    }
}

impl Finalizable for GcsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
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
    pub headers: Vec<(HeaderName, HeaderValue)>,
}

#[derive(Debug)]
pub struct GcsResponse {
    pub inner: http::Response<Body>,
    pub protocol: &'static str,
    pub metadata: RequestMetadata,
}

impl DriverResponse for GcsResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.metadata.event_count(),
            byte_size: self.metadata.events_byte_size(),
            output: None,
        }
    }

    fn bytes_sent(&self) -> Option<BytesSent> {
        Some(BytesSent {
            byte_size: self.metadata.request_encoded_size(),
            protocol: self.protocol,
        })
    }
}

impl Service<GcsRequest> for GcsService {
    type Response = GcsResponse;
    type Error = HttpError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: GcsRequest) -> Self::Future {
        let settings = request.settings;
        let metadata = request.metadata;

        let uri = format!("{}{}", self.base_url, request.key)
            .parse::<Uri>()
            .unwrap();
        let protocol = get_http_scheme_from_uri(&uri);

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
        headers.insert("x-goog-storage-class", settings.storage_class);
        for (p, v) in settings.headers {
            headers.insert(p, v);
        }

        let mut http_request = builder.body(Body::from(request.body)).unwrap();
        if let Some(creds) = &self.creds {
            creds.apply(&mut http_request);
        }

        let mut client = self.client.clone();
        Box::pin(async move {
            let result = client.call(http_request).await;
            result.map(|inner| GcsResponse {
                inner,
                protocol,
                metadata,
            })
        })
    }
}
