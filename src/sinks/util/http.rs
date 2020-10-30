use super::{
    retries::{RetryAction, RetryLogic},
    sink, Batch, TowerBatchedSink, TowerRequestSettings,
};
use crate::{buffers::Acker, event::Event, http::HttpClient};
use bytes::{Buf, Bytes};
use futures::future::BoxFuture;
use futures01::{Async, AsyncSink, Poll as Poll01, Sink, StartSend};
use http::StatusCode;
use hyper::body::{self, Body};
use std::{
    fmt,
    future::Future,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tower::Service;

#[async_trait::async_trait]
pub trait HttpSink: Send + Sync + 'static {
    type Input;
    type Output;

    fn encode_event(&self, event: Event) -> Option<Self::Input>;
    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>>;
}

/// Provides a simple wrapper around internal tower and
/// batching sinks for http.
///
/// This type wraps some `HttpSink` and some `Batch` type
/// and will apply request, batch and tls settings. Internally,
/// it holds an Arc reference to the `HttpSink`. It then exposes
/// a `Sink` interface that can be returned from `SinkConfig`.
///
/// Implementation details we require to buffer a single item due
/// to how `Sink` works. This is because we must "encode" the type
/// to be able to send it to the inner batch type and sink. Because of
/// this we must provide a single buffer slot. To ensure the buffer is
/// fully flushed make sure `poll_complete` returns ready.
pub struct BatchedHttpSink<T, B, L = HttpRetryLogic>
where
    B: Batch,
    B::Output: Clone + Send + 'static,
    L: RetryLogic<Response = http::Response<Bytes>> + Send + 'static,
{
    sink: Arc<T>,
    inner: TowerBatchedSink<
        HttpBatchService<BoxFuture<'static, crate::Result<hyper::Request<Vec<u8>>>>, B::Output>,
        B,
        L,
        B::Output,
    >,
    // An empty slot is needed to buffer an item where we encoded it but
    // the inner sink is applying back pressure. This trick is used in the `WithFlatMap`
    // sink combinator. https://docs.rs/futures/0.1.29/src/futures/sink/with_flat_map.rs.html#20
    slot: Option<B::Input>,
}

impl<T, B> BatchedHttpSink<T, B, HttpRetryLogic>
where
    B: Batch,
    B::Output: Clone + Send + 'static,
    T: HttpSink<Input = B::Input, Output = B::Output>,
{
    pub fn new(
        sink: T,
        batch: B,
        request_settings: TowerRequestSettings,
        batch_timeout: Duration,
        client: HttpClient,
        acker: Acker,
    ) -> Self {
        Self::with_retry_logic(
            sink,
            batch,
            HttpRetryLogic,
            request_settings,
            batch_timeout,
            client,
            acker,
        )
    }
}

impl<T, B, L> BatchedHttpSink<T, B, L>
where
    B: Batch,
    B::Output: Clone + Send + 'static,
    L: RetryLogic<Response = http::Response<Bytes>, Error = hyper::Error> + Send + 'static,
    T: HttpSink<Input = B::Input, Output = B::Output>,
{
    pub fn with_retry_logic(
        sink: T,
        batch: B,
        logic: L,
        request_settings: TowerRequestSettings,
        batch_timeout: Duration,
        client: HttpClient,
        acker: Acker,
    ) -> Self {
        let sink = Arc::new(sink);

        let sink1 = Arc::clone(&sink);
        let request_builder =
            move |b| -> BoxFuture<'static, crate::Result<http::Request<Vec<u8>>>> {
                let sink = Arc::clone(&sink1);
                Box::pin(async move { sink.build_request(b).await })
            };

        let svc = HttpBatchService::new(client, request_builder);
        let inner = request_settings.batch_sink(logic, svc, batch, batch_timeout, acker);

        Self {
            sink,
            inner,
            slot: None,
        }
    }
}

impl<T, B, L> Sink for BatchedHttpSink<T, B, L>
where
    B: Batch,
    B::Output: Clone + Send + 'static,
    T: HttpSink<Input = B::Input, Output = B::Output>,
    L: RetryLogic<Response = http::Response<Bytes>> + Send + 'static,
{
    type SinkItem = crate::Event;
    type SinkError = crate::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.slot.is_some() && self.poll_complete()?.is_not_ready() {
            return Ok(AsyncSink::NotReady(item));
        }
        assert!(self.slot.is_none(), "poll_complete did not clear slot");

        if let Some(item) = self.sink.encode_event(item) {
            self.slot = Some(item);
            self.poll_complete()?;
        }

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll01<(), Self::SinkError> {
        if let Some(item) = self.slot.take() {
            if let AsyncSink::NotReady(item) = self.inner.start_send(item)? {
                self.slot = Some(item);
                return Ok(Async::NotReady);
            }
        }

        self.inner.poll_complete()
    }
}

pub struct HttpBatchService<F, B = Vec<u8>> {
    inner: HttpClient<Body>,
    request_builder: Arc<dyn Fn(B) -> F + Send + Sync>,
}

impl<F, B> HttpBatchService<F, B> {
    pub fn new(
        inner: HttpClient,
        request_builder: impl Fn(B) -> F + Send + Sync + 'static,
    ) -> Self {
        HttpBatchService {
            inner,
            request_builder: Arc::new(Box::new(request_builder)),
        }
    }
}

impl<F, B> Service<B> for HttpBatchService<F, B>
where
    F: Future<Output = crate::Result<hyper::Request<Vec<u8>>>> + Send + 'static,
    B: Send + 'static,
{
    type Response = http::Response<Bytes>;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, body: B) -> Self::Future {
        let request_builder = Arc::clone(&self.request_builder);
        let mut http_client = self.inner.clone();

        Box::pin(async move {
            let request = request_builder(body).await?.map(Body::from);
            let response = http_client.call(request).await?;
            let (parts, body) = response.into_parts();
            let mut body = body::aggregate(body).await?;
            Ok(hyper::Response::from_parts(parts, body.to_bytes()))
        })
    }
}

impl<F, B> Clone for HttpBatchService<F, B> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            request_builder: Arc::clone(&self.request_builder),
        }
    }
}

impl<T: fmt::Debug> sink::Response for http::Response<T> {
    fn is_successful(&self) -> bool {
        self.status().is_success()
    }
}

#[derive(Clone)]
pub struct HttpRetryLogic;

impl RetryLogic for HttpRetryLogic {
    type Error = hyper::Error;
    type Response = hyper::Response<Bytes>;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.status();

        match status {
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(format!(
                "{}: {}",
                status,
                String::from_utf8_lossy(response.body())
            )),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {}", status)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_util::next_addr;
    use futures::{compat::Future01CompatExt, future::ready};
    use futures01::Stream;
    use hyper::{
        service::{make_service_fn, service_fn},
        {Body, Response, Server, Uri},
    };
    use tower::Service;

    #[test]
    fn util_http_retry_logic() {
        let logic = HttpRetryLogic;

        let response_429 = Response::builder().status(429).body(Bytes::new()).unwrap();
        let response_500 = Response::builder().status(500).body(Bytes::new()).unwrap();
        let response_400 = Response::builder().status(400).body(Bytes::new()).unwrap();
        let response_501 = Response::builder().status(501).body(Bytes::new()).unwrap();

        assert!(logic.should_retry_response(&response_429).is_retryable());
        assert!(logic.should_retry_response(&response_500).is_retryable());
        assert!(logic
            .should_retry_response(&response_400)
            .is_not_retryable());
        assert!(logic
            .should_retry_response(&response_501)
            .is_not_retryable());
    }

    #[tokio::test]
    async fn util_http_it_makes_http_requests() {
        let addr = next_addr();

        let uri = format!("http://{}:{}/", addr.ip(), addr.port())
            .parse::<Uri>()
            .unwrap();

        let request = b"hello".to_vec();
        let client = HttpClient::new(None).unwrap();
        let mut service = HttpBatchService::new(client, move |body: Vec<u8>| {
            Box::pin(ready(
                http::Request::post(&uri).body(body).map_err(Into::into),
            ))
        });

        let (tx, rx) = futures01::sync::mpsc::channel(10);

        let new_service = make_service_fn(move |_| {
            let tx = tx.clone();

            let svc = service_fn(move |req| {
                let mut tx = tx.clone();

                async move {
                    let body = hyper::body::aggregate(req.into_body())
                        .await
                        .map_err(|error| format!("error: {}", error))?;
                    let string = String::from_utf8(body.bytes().into())
                        .map_err(|_| "Wasn't UTF-8".to_string())?;
                    tx.try_send(string).map_err(|_| "Send error".to_string())?;

                    Ok::<_, crate::Error>(Response::new(Body::from("")))
                }
            });

            async move { Ok::<_, std::convert::Infallible>(svc) }
        });

        tokio::spawn(async move {
            if let Err(error) = Server::bind(&addr).serve(new_service).await {
                eprintln!("Server error: {}", error);
            }
        });

        tokio::time::delay_for(std::time::Duration::from_millis(50)).await;
        service.call(request).await.unwrap();

        let (body, _rest) = rx.into_future().compat().await.unwrap();
        assert_eq!(body.unwrap(), "hello");
    }
}
