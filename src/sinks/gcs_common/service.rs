use crate::buffers::Ackable;
use crate::event::{EventFinalizers, EventStatus, Finalizable};
use crate::http::{HttpClient, HttpError};
use crate::sinks::gcp::GcpCredentials;
use bytes::Bytes;
use futures::future::BoxFuture;
use http::{
    header::{HeaderName, HeaderValue},
    Request, Uri,
};
use hyper::Body;
use std::task::Poll;
use tower::Service;

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
    pub body: Bytes,
    pub key: String,
    pub settings: GcsRequestSettings,
    pub finalizers: EventFinalizers,
}

impl Ackable for GcsRequest {
    fn ack_size(&self) -> usize {
        self.body.len()
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
    pub metadata: Vec<(HeaderName, HeaderValue)>,
}

#[derive(Debug)]
pub struct GcsResponse {
    pub inner: http::Response<Body>,
}

impl AsRef<EventStatus> for GcsResponse {
    fn as_ref(&self) -> &EventStatus {
        &EventStatus::Delivered
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

        let uri = format!("{}{}", self.base_url, request.key)
            .parse::<Uri>()
            .unwrap();
        let mut builder = Request::put(uri);
        let headers = builder.headers_mut().unwrap();
        headers.insert("content-type", settings.content_type);
        headers.insert(
            "content-length",
            HeaderValue::from_str(&format!("{}", request.body.len())).unwrap(),
        );
        settings
            .content_encoding
            .map(|ce| headers.insert("content-encoding", ce));
        settings.acl.map(|acl| headers.insert("x-goog-acl", acl));
        headers.insert("x-goog-storage-class", settings.storage_class);
        for (p, v) in settings.metadata {
            headers.insert(p, v);
        }

        let mut request = builder.body(Body::from(request.body)).unwrap();
        if let Some(creds) = &self.creds {
            creds.apply(&mut request);
        }

        let mut client = self.client.clone();
        Box::pin(async move {
            let result = client.call(request).await;
            result.map(|inner| GcsResponse { inner })
        })
    }
}
