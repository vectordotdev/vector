#![warn(clippy::pedantic)]

use std::{
    marker::PhantomData,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::{Bytes, BytesMut};
use futures::future::BoxFuture;
use http_1::{Request, Response, Uri};
use http_body_util::BodyExt;
use tower::Service;
#[cfg(any(
    feature = "sinks-gcp",
    feature = "sinks-greptimedb_logs",
    feature = "sinks-honeycomb",
    feature = "sinks-keep",
))]
use tracing::debug;

use vector_lib::{
    event::EventStatus,
    request_metadata::{GroupedCountByteSize, MetaDescriptive},
    stream::DriverResponse,
};

use crate::{
    http::client_v1::{HttpClient, HttpError, full_body},
    internal_events::{EndpointBytesSent, SinkRequestBuildError},
    sinks::util::{
        http::{HttpRequest, RESPONSE_BODY_BUDGET_BYTES, ResponseBodyBudget},
        sink,
    },
};

/// Builds a native `http 1` request from a buffered HTTP sink request.
pub(crate) trait HttpServiceRequestBuilder<T: Send> {
    fn build(&self, request: HttpRequest<T>) -> Result<Request<Bytes>, crate::Error>;
}

/// The response returned by the native `http 1` sink service.
#[derive(Debug)]
pub(crate) struct HttpResponse {
    pub response: Response<Bytes>,
    pub events_byte_size: GroupedCountByteSize,
    pub raw_byte_size: usize,
}

impl sink::Response for HttpResponse {
    fn is_successful(&self) -> bool {
        self.response.status().is_success()
    }

    fn is_transient(&self) -> bool {
        self.response.status().is_server_error()
    }
}

impl DriverResponse for HttpResponse {
    fn event_status(&self) -> EventStatus {
        if self.response.status().is_success() {
            EventStatus::Delivered
        } else if self.response.status().is_server_error() {
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
    budget: Arc<ResponseBodyBudget>,
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
            budget: ResponseBodyBudget::new(RESPONSE_BODY_BUDGET_BYTES),
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

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Apply backpressure while in-flight responses hold the response body budget:
        // new batches are not admitted until buffered responses complete. Register before
        // re-checking so a release between the check and registration is not missed.
        if self.budget.is_exhausted() {
            self.budget.register_waker(cx.waker());
            if self.budget.is_exhausted() {
                return Poll::Pending;
            }
        }
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: HttpRequest<T>) -> Self::Future {
        let request_builder = Arc::clone(&self.request_builder);
        let client = self.client.clone();
        let budget = Arc::clone(&self.budget);
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
            let body = collect_response_body(body, &budget).await?;
            Ok(HttpResponse {
                response: Response::from_parts(parts, body),
                events_byte_size,
                raw_byte_size,
            })
        })
    }
}

/// Collects an HTTP response body into a single contiguous buffer, pacing reads against the
/// sink's [`ResponseBodyBudget`].
///
/// The full body is always returned. The budget bounds the aggregate memory the sink buffers
/// across in-flight requests by pausing reads — which stalls the remote endpoint via TCP flow
/// control — rather than by truncating.
async fn collect_response_body<B>(
    body: B,
    budget: &Arc<ResponseBodyBudget>,
) -> Result<Bytes, HttpError>
where
    B: http_body_1::Body<Data = Bytes>,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let mut body = std::pin::pin!(body);
    let mut hold = budget.hold();
    let mut buffer = BytesMut::new();
    while let Some(frame) = body.as_mut().frame().await {
        let frame = frame.map_err(HttpError::new)?;
        let Ok(chunk) = frame.into_data() else {
            // Trailers frame: no body content, keep reading.
            continue;
        };
        hold.record(chunk.len()).await;
        buffer.extend_from_slice(&chunk);
    }
    Ok(buffer.freeze())
}

// Clickhouse implements its own retry logic, so this is only used by the sinks below.
#[cfg(any(
    feature = "sinks-gcp",
    feature = "sinks-greptimedb_logs",
    feature = "sinks-honeycomb",
    feature = "sinks-keep",
))]
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
            let status = http::StatusCode::from_u16(response.response.status().as_u16())
                .expect("HTTP status codes are valid u16 values");
            if !status.is_success() {
                debug!(
                    message = "HTTP response.",
                    %status,
                    body = %String::from_utf8_lossy(
                        &response.response.body()[..response
                            .response
                            .body()
                            .len()
                            .min(super::http::MAX_ERROR_BODY_BYTES)],
                    ),
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

#[cfg(test)]
mod test {
    use std::convert::Infallible;

    use http_body_util::{Full, StreamBody};

    use super::*;

    fn multi_frame_body(
        chunks: &'static [&'static [u8]],
    ) -> impl http_body_1::Body<Data = Bytes, Error = Infallible> {
        let frames = chunks
            .iter()
            .map(|chunk| Ok(http_body_1::Frame::data(Bytes::from_static(chunk))));
        StreamBody::new(futures::stream::iter(frames))
    }

    #[tokio::test]
    async fn collect_response_body_collects_full_body_under_budget() {
        let budget = ResponseBodyBudget::new(100);
        let body = multi_frame_body(&[b"aaaa", b"bbbb", b"cccc"]);

        let collected = collect_response_body(body, &budget).await.unwrap();
        assert_eq!(&collected[..], b"aaaabbbbcccc");
        assert_eq!(budget.used(), 0, "budget must be released on completion");
    }

    #[tokio::test]
    async fn collect_response_body_sole_holder_may_exceed_budget() {
        // A single response larger than the whole budget still completes instead of deadlocking.
        let budget = ResponseBodyBudget::new(4);
        let collected = collect_response_body(Full::new(Bytes::from("0123456789")), &budget)
            .await
            .unwrap();
        assert_eq!(&collected[..], b"0123456789");
        assert_eq!(budget.used(), 0);
    }
}
