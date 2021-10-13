use crate::{
    http::{HttpClient, HttpError, BuildRequest, CallRequest},
    sinks::util::Compression,
};
use bytes::{Buf, Bytes};
use futures::future::BoxFuture;
use http::{Request, Response, StatusCode, Uri, header::{CONTENT_ENCODING, CONTENT_TYPE, HeaderValue}};
use hyper::Body;
use snafu::ResultExt;
use std::task::{Context, Poll};
use tower::Service;
use vector_core::{buffers::Ackable, event::{EventFinalizers, EventStatus, Finalizable}};

#[derive(Debug, Clone)]
pub struct DatadogMetricsRequest {
    pub payload: Bytes,
    pub uri: Uri,
    pub content_type: &'static str,
    pub compression: Compression,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
}

impl DatadogMetricsRequest {
    pub fn into_http_request(self, api_key: HeaderValue) -> http::Result<Request<Body>> {
        let content_encoding = self.compression.content_encoding();

        let request = Request::post(self.uri)
            .header(CONTENT_TYPE, self.content_type)
            .header("DD-API-KEY", api_key);

        let request = if let Some(value) = content_encoding {
            request.header(CONTENT_ENCODING, value)
        } else {
            request
        };

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

#[derive(Debug)]
pub struct DatadogMetricsResponse {
    status_code: StatusCode,
}

impl From<Response<Body>> for DatadogMetricsResponse {
    fn from(response: Response<Body>) -> DatadogMetricsResponse {
        DatadogMetricsResponse {
            status_code: response.status(),
        }
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
    api_key: HeaderValue,
}

impl DatadogMetricsService {
    pub fn new(client: HttpClient, api_key: &str) -> Self {
        DatadogMetricsService {
            client,
            api_key: HeaderValue::from_str(api_key)
                .expect("API key should be only valid ASCII characters"),
        }
    }
}

impl Service<DatadogMetricsRequest> for DatadogMetricsService {
    type Response = http::Response<Bytes>;
    type Error = HttpError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.client.poll_ready(cx)
    }

    fn call(&mut self, request: DatadogMetricsRequest) -> Self::Future {
        let client = self.client.clone();
        let api_key = self.api_key.clone();

        Box::pin(async move {
            let request = request.into_http_request(api_key)
                .context(BuildRequest)?;
            let response = client.send(request).await?;
            let (parts, body) = response.into_parts();
            let mut body = hyper::body::aggregate(body).await
                .context(CallRequest)?;

            Ok(hyper::Response::from_parts(
                parts,
                body.copy_to_bytes(body.remaining()),
            ))
        })
    }
}
