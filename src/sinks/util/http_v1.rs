#![allow(dead_code)] // This shared service is exercised as sinks migrate to the native client.

use std::{
    marker::PhantomData,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use http_1::{Request, Response, Uri};
use http_body_util::BodyExt;
use tower::Service;
use tracing::debug;

use vector_lib::{
    event::EventStatus,
    request_metadata::{GroupedCountByteSize, MetaDescriptive},
    stream::DriverResponse,
};

use crate::{
    http::client_v1::{HttpClient, HttpError, full_body},
    internal_events::{EndpointBytesSent, SinkRequestBuildError},
    sinks::util::{http::HttpRequest, sink},
};

/// Builds a native `http 1` request from a buffered HTTP sink request.
pub(crate) trait HttpServiceRequestBuilder<T: Send> {
    fn build(&self, request: HttpRequest<T>) -> Result<Request<Bytes>, crate::Error>;
}

/// The response returned by the native `http 1` sink service.
#[derive(Debug)]
pub(crate) struct HttpResponse {
    pub http_response: Response<Bytes>,
    pub events_byte_size: GroupedCountByteSize,
    pub raw_byte_size: usize,
}

impl sink::Response for HttpResponse {
    fn is_successful(&self) -> bool {
        self.http_response.status().is_success()
    }

    fn is_transient(&self) -> bool {
        self.http_response.status().is_server_error()
    }
}

impl DriverResponse for HttpResponse {
    fn event_status(&self) -> EventStatus {
        if self.http_response.status().is_success() {
            EventStatus::Delivered
        } else if self.http_response.status().is_server_error() {
            EventStatus::Errored
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

/// Native `http 1` equivalent of the legacy `HttpService`.
#[derive(Clone)]
pub(crate) struct HttpService<B, T: Send> {
    client: HttpClient,
    request_builder: Arc<B>,
    _phantom: PhantomData<fn() -> T>,
}

impl<B, T> HttpService<B, T>
where
    B: HttpServiceRequestBuilder<T> + Send + Sync + 'static,
    T: Send + 'static,
{
    pub(crate) fn new(client: HttpClient, request_builder: B) -> Self {
        Self {
            client,
            request_builder: Arc::new(request_builder),
            _phantom: PhantomData,
        }
    }
}

impl<B, T> Service<HttpRequest<T>> for HttpService<B, T>
where
    B: HttpServiceRequestBuilder<T> + Send + Sync + 'static,
    T: Send + 'static,
{
    type Response = HttpResponse;
    type Error = HttpError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: HttpRequest<T>) -> Self::Future {
        let request_builder = Arc::clone(&self.request_builder);
        let client = self.client.clone();
        let metadata = std::mem::take(request.metadata_mut());
        let raw_byte_size = metadata.request_encoded_size();
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();

        Box::pin(async move {
            let request = request_builder
                .build(request)
                .inspect_err(|error| {
                    emit!(SinkRequestBuildError { error });
                })
                .map_err(HttpError::from)?;
            let byte_size = request.body().len();
            let (protocol, endpoint) = protocol_endpoint(request.uri());
            let response = client.send(request.map(full_body)).await?;

            if response.status().is_success() {
                emit!(EndpointBytesSent {
                    byte_size,
                    protocol: &protocol,
                    endpoint: &endpoint,
                });
            }

            let (parts, body) = response.into_parts();
            let body = body.collect().await.map_err(HttpError::new)?.to_bytes();
            Ok(HttpResponse {
                http_response: Response::from_parts(parts, body),
                events_byte_size,
                raw_byte_size,
            })
        })
    }
}

pub(crate) fn http_response_retry_logic<Request: Clone + Send + Sync + 'static>(
    retry_strategy: super::http::RetryStrategy,
) -> super::http::HttpStatusRetryLogic<
    impl Fn(&HttpResponse) -> http::StatusCode + Clone + Send + Sync + 'static,
    Request,
    HttpResponse,
    HttpError,
> {
    super::http::HttpStatusRetryLogic::new_with_error(
        |response: &HttpResponse| {
            let status = http::StatusCode::from_u16(response.http_response.status().as_u16())
                .expect("HTTP status codes are valid u16 values");
            if !status.is_success() {
                debug!(
                    message = "HTTP response.",
                    %status,
                    body = %String::from_utf8_lossy(response.http_response.body()),
                );
            }
            status
        },
        retry_strategy,
    )
}

fn protocol_endpoint(uri: &Uri) -> (String, String) {
    let uri = uri
        .to_string()
        .parse::<http::Uri>()
        .expect("a valid HTTP/1 URI is valid as an HTTP URI");
    super::uri::protocol_endpoint(uri)
}
