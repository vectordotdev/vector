//! Service implementation for the `http` sink.

use std::{
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use http::{HeaderName, HeaderValue, Method, Request, Response, Uri};
use hyper::Body;
use indexmap::IndexMap;

use crate::{
    http::{Auth, HttpClient},
    sinks::{
        prelude::*,
        util::{http::HttpBatchService, UriSerde},
    },
};

use super::config::HttpMethod;

#[derive(Clone)]
pub(super) struct HttpRequest {
    payload: Bytes,
    finalizers: EventFinalizers,
    request_metadata: RequestMetadata,
}

impl HttpRequest {
    /// Creates a new `HttpRequest`.
    pub(super) fn new(
        payload: Bytes,
        finalizers: EventFinalizers,
        request_metadata: RequestMetadata,
    ) -> Self {
        Self {
            payload,
            finalizers,
            request_metadata,
        }
    }
}

impl Finalizable for HttpRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for HttpRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

impl ByteSizeOf for HttpRequest {
    fn allocated_bytes(&self) -> usize {
        self.payload.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

/// Response type for use in the `Service` implementation of HTTP stream sinks.
pub(super) struct HttpResponse {
    pub(super) http_response: Response<Bytes>,
    events_byte_size: GroupedCountByteSize,
    raw_byte_size: usize,
}

impl DriverResponse for HttpResponse {
    fn event_status(&self) -> EventStatus {
        if self.http_response.status().is_success() {
            EventStatus::Delivered
        } else {
            EventStatus::Rejected
        }
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.raw_byte_size)
    }
}

#[derive(Debug, Clone)]
pub(super) struct HttpSinkRequestBuilder {
    uri: UriSerde,
    method: HttpMethod,
    auth: Option<Auth>,
    headers: IndexMap<HeaderName, HeaderValue>,
    content_type: Option<String>,
    content_encoding: Option<String>,
}

impl HttpSinkRequestBuilder {
    /// Creates a new `HttpSinkRequestBuilder`
    pub(super) const fn new(
        uri: UriSerde,
        method: HttpMethod,
        auth: Option<Auth>,
        headers: IndexMap<HeaderName, HeaderValue>,
        content_type: Option<String>,
        content_encoding: Option<String>,
    ) -> Self {
        Self {
            uri,
            method,
            auth,
            headers,
            content_type,
            content_encoding,
        }
    }

    fn build(&self, body: Bytes) -> Request<Bytes> {
        let method: Method = self.method.into();
        let uri: Uri = self.uri.uri.clone();
        let mut builder = Request::builder().method(method).uri(uri);

        if let Some(content_type) = &self.content_type {
            builder = builder.header("Content-Type", content_type);
        }

        if let Some(content_encoding) = &self.content_encoding {
            builder = builder.header("Content-Encoding", content_encoding);
        }

        let headers = builder
            .headers_mut()
            // The request building should not have errors at this point, and if it did it would fail in the call to `body()` also.
            .expect("Failed to access headers in http::Request builder- builder has errors.");

        for (header, value) in self.headers.iter() {
            headers.insert(header, value.clone());
        }

        // The request building should not have errors at this point
        let mut request = builder
            .body(body)
            .expect("Failed to assign body to request- builder has errors");

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        request
    }
}

#[derive(Clone)]
pub(super) struct HttpService {
    batch_service:
        HttpBatchService<BoxFuture<'static, Result<Request<Bytes>, crate::Error>>, HttpRequest>,
}

impl HttpService {
    pub(super) fn new(
        http_client: HttpClient<Body>,
        http_request_builder: HttpSinkRequestBuilder,
    ) -> Self {
        let http_request_builder = Arc::new(http_request_builder);

        let batch_service = HttpBatchService::new(http_client, move |req| {
            let req: HttpRequest = req;

            let request_builder = Arc::clone(&http_request_builder);

            let fut: BoxFuture<'static, Result<http::Request<Bytes>, crate::Error>> =
                Box::pin(async move { Ok(request_builder.build(req.payload)) });

            fut
        });
        Self { batch_service }
    }
}

impl Service<HttpRequest> for HttpService {
    type Response = HttpResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: HttpRequest) -> Self::Future {
        let mut http_service = self.batch_service.clone();

        let raw_byte_size = request.payload.len();
        let metadata = std::mem::take(request.metadata_mut());
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();

        Box::pin(async move {
            let http_response = http_service.call(request).await?;

            Ok(HttpResponse {
                http_response,
                events_byte_size,
                raw_byte_size,
            })
        })
    }
}
