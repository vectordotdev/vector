use crate::{
    http::{BuildRequest, CallRequest, HttpClient, HttpError},
    sinks::util::{
        retries::{RetryAction, RetryLogic},
    },
};
use bytes::{Buf, Bytes};
use futures::future::BoxFuture;
use http::{
    header::{HeaderValue, CONTENT_ENCODING, CONTENT_TYPE},
    Request, Response, StatusCode, Uri,
};
use hyper::Body;
use snafu::ResultExt;
use std::task::{Context, Poll};
use tower::Service;
use vector_core::{
    buffers::Ackable,
    event::{EventFinalizers, EventStatus, Finalizable},
};

#[derive(Debug, Default, Clone)]
pub struct DatadogMetricsRetryLogic;

impl RetryLogic for DatadogMetricsRetryLogic {
    type Error = HttpError;
    type Response = DatadogMetricsResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.status_code;

        match status {
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(
                format!("{}: {}", status, String::from_utf8_lossy(&response.body)).into(),
            ),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {}", status).into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DatadogMetricsRequest {
    pub payload: Bytes,
    pub uri: Uri,
    pub content_type: &'static str,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
}

impl DatadogMetricsRequest {
    pub fn into_http_request(self, api_key: HeaderValue) -> http::Result<Request<Body>> {
        let request = Request::post(self.uri)
            .header(CONTENT_TYPE, self.content_type)
            .header("DD-API-KEY", api_key)
            .header(CONTENT_ENCODING, "deflate");

        // TODO: do we need the agent proto payload repo version as a header for sketches? or is
        // that just a nice-to-have? not clear yet.
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
    body: Bytes,
}

impl From<Response<Bytes>> for DatadogMetricsResponse {
    fn from(response: Response<Bytes>) -> DatadogMetricsResponse {
        let (parts, body) = response.into_parts();
        DatadogMetricsResponse {
            status_code: parts.status,
            body,
        }
    }
}

impl AsRef<EventStatus> for DatadogMetricsResponse {
    fn as_ref(&self) -> &EventStatus {
        if self.status_code.is_success() {
            &EventStatus::Delivered
        } else if self.status_code.is_client_error() {
            &EventStatus::Failed
        } else {
            &EventStatus::Errored
        }
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
    type Response = DatadogMetricsResponse;
    type Error = HttpError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.client.poll_ready(cx)
    }

    fn call(&mut self, request: DatadogMetricsRequest) -> Self::Future {
        let client = self.client.clone();
        let api_key = self.api_key.clone();

        Box::pin(async move {
            let request = request.into_http_request(api_key).context(BuildRequest)?;
            let response = client.send(request).await?;
            let (parts, body) = response.into_parts();
            let mut body = hyper::body::aggregate(body).await.context(CallRequest)?;

            let response = hyper::Response::from_parts(parts, body.copy_to_bytes(body.remaining()));
            Ok(response.into())
        })
    }
}
