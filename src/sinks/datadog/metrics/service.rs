use crate::{
    http::{HttpClient, HttpError},
    sinks::util::{
        retries::{RetryAction, RetryLogic},
        Compression,
    },
};
use bytes::Bytes;
use futures::future::BoxFuture;
use http::{
    header::{CONTENT_ENCODING, CONTENT_TYPE},
    Request, Response, Uri,
};
use hyper::Body;
use std::{
    sync::Arc,
    task::{Context, Poll},
};
use tower::Service;
use vector_core::event::{EventFinalizers, EventStatus, Finalizable};

#[derive(Clone)]
pub struct DatadogMetricsRetryLogic;

impl RetryLogic for DatadogMetricsRetryLogic {
    type Error = HttpError;
    type Response = DatadogMetricsResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        todo!()
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct DatadogMetricsRequest {
    pub body: Bytes,
    pub uri: Uri,
    pub api_key: Arc<str>,
    pub compression: Compression,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
}

impl DatadogMetricsRequest {
    pub fn into_http_request(self) -> crate::Result<Request<Body>> {
        let content_encoding = self.compression.content_encoding();

        let request = Request::post(self.uri)
            .header("DD-API-KEY", self.api_key.as_ref())
            .header(CONTENT_TYPE, "application/json");

        let request = if let Some(value) = content_encoding {
            request.header(CONTENT_ENCODING, value)
        } else {
            request
        };

        request.body(Body::from(self.body)).map_err(Into::into)
    }
}

impl Finalizable for DatadogMetricsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

#[derive(Debug)]
pub struct DatadogMetricsResponse;

impl From<Response<Body>> for DatadogMetricsResponse {
    fn from(_response: Response<Body>) -> DatadogMetricsResponse {
        DatadogMetricsResponse
    }
}

impl AsRef<EventStatus> for DatadogMetricsResponse {
    fn as_ref(&self) -> &EventStatus {
        &EventStatus::Delivered
    }
}

#[derive(Clone)]
pub struct DatadogMetricsService {
    client: HttpClient,
}

impl DatadogMetricsService {
    pub const fn new(client: HttpClient) -> Self {
        DatadogMetricsService { client }
    }
}

impl Service<DatadogMetricsRequest> for DatadogMetricsService {
    type Response = DatadogMetricsResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: DatadogMetricsRequest) -> Self::Future {
        let client = self.client.clone();
        Box::pin(async move {
            let request = request.into_http_request()?;

            client
                .call(request)
                .await
                .map(DatadogMetricsResponse::from)
                .map_err(Into::into)
        })
    }
}
