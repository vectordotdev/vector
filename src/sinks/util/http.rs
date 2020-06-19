use super::{
    retries2::{RetryAction, RetryLogic},
    service2::{TowerBatchedSink, TowerRequestSettings},
    sink, Batch, BatchSettings,
};
use crate::{
    dns::Resolver,
    event::Event,
    tls::{tls_connector_builder, MaybeTlsSettings},
    topology::config::SinkContext,
};
use bytes05::{Buf, Bytes};
use futures::future::BoxFuture;
use futures01::{Async, AsyncSink, Poll as Poll01, Sink, StartSend};
use http02::header::HeaderValue;
use http02::{Request, StatusCode};
use hyper::body::{self, Body, HttpBody};
use hyper::client::HttpConnector;
use hyper::Client;
use hyper_openssl::HttpsConnector;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    sync::Arc,
    task::{Context, Poll},
};
use tower03::Service;
use tracing::Span;
use tracing_futures::Instrument;

pub type Response = http02::Response<Bytes>;
pub type Error = hyper::Error;
pub type HttpClientFuture = <HttpClient as Service<http02::Request<Body>>>::Future;

pub trait HttpSink: Send + Sync + 'static {
    type Input;
    type Output;

    fn encode_event(&self, event: Event) -> Option<Self::Input>;
    fn build_request(&self, events: Self::Output) -> http02::Request<Vec<u8>>;
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
    L: RetryLogic<Response = http02::Response<Bytes>> + Send + 'static,
{
    sink: Arc<T>,
    inner: TowerBatchedSink<HttpBatchService<B::Output>, B, L, B::Output>,
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
        batch_settings: BatchSettings,
        tls_settings: impl Into<MaybeTlsSettings>,
        cx: &SinkContext,
    ) -> Self {
        Self::with_retry_logic(
            sink,
            batch,
            HttpRetryLogic,
            request_settings,
            batch_settings,
            tls_settings,
            cx,
        )
    }
}

impl<T, B, L> BatchedHttpSink<T, B, L>
where
    B: Batch,
    B::Output: Clone + Send + 'static,
    L: RetryLogic<Response = http02::Response<Bytes>, Error = hyper::Error> + Send + 'static,
    T: HttpSink<Input = B::Input, Output = B::Output>,
{
    pub fn with_retry_logic(
        sink: T,
        batch: B,
        logic: L,
        request_settings: TowerRequestSettings,
        batch_settings: BatchSettings,
        tls_settings: impl Into<MaybeTlsSettings>,
        cx: &SinkContext,
    ) -> Self {
        let sink = Arc::new(sink);
        let sink1 = sink.clone();
        let svc =
            HttpBatchService::new(cx.resolver(), tls_settings, move |b| sink1.build_request(b));

        let inner = request_settings.batch_sink(logic, svc, batch, batch_settings, cx.acker());

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
    L: RetryLogic<Response = http02::Response<Bytes>> + Send + 'static,
{
    type SinkItem = crate::Event;
    type SinkError = crate::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.slot.is_some() {
            if self.poll_complete()?.is_not_ready() {
                return Ok(AsyncSink::NotReady(item));
            }
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

pub struct HttpClient<B = Body> {
    client: Client<HttpsConnector<HttpConnector<Resolver>>, B>,
    span: Span,
    user_agent: HeaderValue,
}

impl<B> HttpClient<B>
where
    B: HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error>,
{
    pub fn new(
        resolver: Resolver,
        tls_settings: impl Into<MaybeTlsSettings>,
    ) -> crate::Result<HttpClient<B>> {
        let mut http = HttpConnector::new_with_resolver(resolver.clone());
        http.enforce_http(false);

        let settings = tls_settings.into();
        let tls = tls_connector_builder(&settings)?;
        let mut https = HttpsConnector::with_connector(http, tls)?;

        let settings = settings.tls().cloned();
        https.set_callback(move |c, _uri| {
            if let Some(settings) = &settings {
                settings.apply_connect_configuration(c);
            }

            Ok(())
        });

        let client = Client::builder().build(https);

        let version = crate::get_version();
        let user_agent = HeaderValue::from_str(&format!("Vector/{}", version))
            .expect("Invalid header value for version!");

        let span = tracing::info_span!("http");

        Ok(HttpClient {
            client,
            span,
            user_agent,
        })
    }

    pub async fn send(&mut self, request: Request<B>) -> crate::Result<http02::Response<Body>> {
        self.call(request).await.map_err(Into::into)
    }
}

impl<B> Service<Request<B>> for HttpClient<B>
where
    B: HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<crate::Error>,
{
    type Response = http02::Response<Body>;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, mut request: Request<B>) -> Self::Future {
        let _enter = self.span.enter();

        if !request.headers().contains_key("User-Agent") {
            request
                .headers_mut()
                .insert("User-Agent", self.user_agent.clone());
        }

        debug!(message = "sending request.", uri = %request.uri(), method = %request.method());

        let response = self.client.request(request);

        let fut = async move {
            let res = response.await?;
            debug!(
                    message = "response.",
                    status = ?res.status(),
                    version = ?res.version(),
            );
            Ok(res)
        }
        .instrument(self.span.clone());

        Box::pin(fut)
    }
}

impl<B> Clone for HttpClient<B> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            span: self.span.clone(),
            user_agent: self.user_agent.clone(),
        }
    }
}

impl<B> fmt::Debug for HttpClient<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpClient")
            .field("client", &self.client)
            .field("user_agent", &self.user_agent)
            .finish()
    }
}

#[derive(Clone)]
pub struct HttpBatchService<B = Vec<u8>> {
    inner: HttpClient<Body>,
    request_builder: Arc<dyn Fn(B) -> hyper::Request<Vec<u8>> + Sync + Send>,
}

impl<B> HttpBatchService<B> {
    pub fn new(
        resolver: Resolver,
        tls_settings: impl Into<MaybeTlsSettings>,
        request_builder: impl Fn(B) -> hyper::Request<Vec<u8>> + Sync + Send + 'static,
    ) -> HttpBatchService<B> {
        let inner =
            HttpClient::new(resolver, tls_settings).expect("Unable to initialize http client");

        HttpBatchService {
            inner,
            request_builder: Arc::new(Box::new(request_builder)),
        }
    }
}

impl<B> Service<B> for HttpBatchService<B> {
    type Response = Response;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, body: B) -> Self::Future {
        let request = (self.request_builder)(body).map(Body::from);

        let response = self.inner.call(request);
        let fut = async move {
            let res = response.await?;
            let (parts, body) = res.into_parts();
            let mut body = body::aggregate(body).await?;
            Ok(hyper::Response::from_parts(parts, body.to_bytes()))
        };

        Box::pin(fut)
    }
}

impl<T: fmt::Debug> sink::Response for http02::Response<T> {
    fn is_successful(&self) -> bool {
        self.status().is_success()
    }
}
#[derive(Clone)]
pub struct HttpRetryLogic;

impl RetryLogic for HttpRetryLogic {
    type Error = hyper::Error;
    type Response = hyper::Response<Bytes>;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_connect() || error.is_closed()
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.status();

        match status {
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("Too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(
                format!("{}: {}", status, String::from_utf8_lossy(response.body())).into(),
            ),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {}", status)),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
pub enum Auth {
    Basic { user: String, password: String },
    Bearer { token: String },
}

impl Auth {
    pub fn apply<B>(&self, req: &mut Request<B>) {
        use headers03::HeaderMapExt;

        match &self {
            Auth::Basic { user, password } => {
                let auth = headers03::Authorization::basic(&user, &password);
                req.headers_mut().typed_insert(auth);
            }
            Auth::Bearer { token } => match headers03::Authorization::bearer(&token) {
                Ok(auth) => req.headers_mut().typed_insert(auth),
                Err(error) => error!(message = "invalid bearer token", %token, %error),
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_util::runtime;
    use bytes05::Buf;
    use futures01::{Future, Stream};
    use http02::Method;
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Body, Response, Server, Uri};
    use tower03::Service;

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

    #[test]
    fn util_http_it_makes_http_requests() {
        let addr = crate::test_util::next_addr();
        let resolver = Resolver;

        let uri = format!("http://{}:{}/", addr.ip(), addr.port())
            .parse::<Uri>()
            .unwrap();

        let request = b"hello".to_vec();
        let mut service = HttpBatchService::new(resolver, None, move |body: Vec<u8>| {
            Request::builder()
                .method(Method::POST)
                .uri(uri.clone())
                .body(body.into())
                .unwrap()
        });

        let (tx, rx) = futures01::sync::mpsc::channel(10);

        let new_service = make_service_fn(move |_| {
            let tx = tx.clone();

            let svc = service_fn(move |req| {
                let mut tx = tx.clone();

                async move {
                    let body = hyper::body::aggregate(req.into_body())
                        .await
                        .map_err(|e| format!("error: {}", e))?;
                    let string = String::from_utf8(body.bytes().into())
                        .map_err(|_| "Wasn't UTF-8".to_string())?;
                    tx.try_send(string).map_err(|_| "Send error".to_string())?;

                    Ok::<_, crate::Error>(Response::new(Body::from("")))
                }
            });

            async move { Ok::<_, std::convert::Infallible>(svc) }
        });

        let mut rt = runtime();

        rt.spawn_std(async move {
            if let Err(e) = Server::bind(&addr).serve(new_service).await {
                eprintln!("server error: {}", e);
            }
        });

        rt.block_on_std(async move {
            tokio::time::delay_for(std::time::Duration::from_millis(50)).await;
            service.call(request).await
        })
        .unwrap();

        let _ = rt.shutdown_now();

        let (body, _rest) = rx.into_future().wait().unwrap();
        assert_eq!(body.unwrap(), "hello");
    }
}
