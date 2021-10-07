use crate::{http::{HttpClient, HttpError}, internal_events::aws_s3::sink::S3EventsSent, serde::to_string, sinks::util::{Compression, retries::RetryLogic}};
use bytes::Bytes;
use futures::{future::BoxFuture, stream};
use http::{Method, Request, Uri, header::{CONTENT_ENCODING, CONTENT_TYPE}, request::Builder};
use hyper::Body;
use md5::Digest;
use mime::APPLICATION_JSON;
use rusoto_core::{ByteStream, RusotoError};
use rusoto_s3::{PutObjectError, PutObjectOutput, PutObjectRequest, S3Client, S3};
use std::{sync::Arc, task::{Context, Poll}};
use tower::Service;
use tracing_futures::Instrument;
use vector_core::event::{EventFinalizers, EventStatus, Finalizable};

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
			.header("DD-API-KEY", self.api_key.as_str())
			.header(CONTENT_TYPE, APPLICATION_JSON);

		let request = if let Some(value) = content_encoding {
			request.header(CONTENT_ENCODING, value)
		} else {
			request
		};

		request.body(self.body).map_err(Into::into)
	}
}

impl Finalizable for DatadogMetricsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

#[derive(Debug)]
pub struct DatadogMetricsResponse {
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
        let content_encoding = request.compression.content_encoding();

		let request = Request::post(request.uri)
			.header("DD-API-KEY", request.api_key.as_str())
			.header(CONTENT_TYPE, APPLICATION_JSON);

		let request = if let Some(value) = content_encoding {
			request.header(CONTENT_ENCODING, value)
		} else {
			request
		};

		let client = self.client.clone();
        Box::pin(async move {
			let request = request.into_http_request()?;

			client.call(request)
				.await
				.map(DatadogMetricsResponse::from)
				.amp_err(Into::into)
        })
    }
}

fn bytes_to_bytestream(buf: Bytes) -> ByteStream {
    // We _have_ to provide the size hint, because without it, Rusoto can't
    // generate the Content-Length header which is required for the S3 PutObject
    // API call.
    let len = buf.len();
    ByteStream::new_with_size(Box::pin(stream::once(async move { Ok(buf) })), len)
}
