use super::{
    retries::{RetryAction, RetryLogic},
    service::{TowerBatchedSink, TowerRequestSettings},
    tls::{TlsConnectorExt, TlsSettings},
    Batch, BatchSettings, BatchSink,
};
use crate::dns::Resolver;
use crate::event::Event;
use crate::topology::config::SinkContext;
use bytes::Bytes;
use futures::{AsyncSink, Future, Poll, Sink, StartSend, Stream};
use http::{Request, StatusCode};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::executor::DefaultExecutor;
use tower::Service;
use tower_hyper::client::Client;
use tracing::field;
use tracing_tower::{InstrumentableService, InstrumentedService};

pub type Response = hyper::Response<Bytes>;
pub type Error = hyper::Error;

pub trait HttpSink: Send + Sync + 'static {
    type Input;
    type Output;

    fn encode_event(&self, event: Event) -> Option<Self::Input>;
    fn build_request(&self, events: Self::Output) -> http::Request<Vec<u8>>;
}

pub struct BatchedHttpSink<T, B: Batch>
where
    B::Output: Clone,
{
    sink: Arc<T>,
    inner: BatchSink<B, TowerBatchedSink<B::Output, HttpRetryLogic, B, HttpService<B::Output>>>,
    // An empty slot is needed to buffer an item where we encoded it but
    // the inner sink is applying back pressure. This trick is used in the `WithFlatMap`
    // sink combinator. https://docs.rs/futures/0.1.29/src/futures/sink/with_flat_map.rs.html#20
    slot: Option<B::Input>,
}

impl<T, B> BatchedHttpSink<T, B>
where
    B: Batch,
    B::Output: Clone,
    T: HttpSink<Input = B::Input, Output = B::Output>,
{
    pub fn new(
        sink: T,
        batch: B,
        request_settings: TowerRequestSettings,
        batch_settings: BatchSettings,
        tls_settings: Option<TlsSettings>,
        cx: &SinkContext,
    ) -> Self {
        let sink = Arc::new(sink);
        let sink1 = sink.clone();
        let svc = HttpService::new2(cx.resolver(), tls_settings, move |b| sink1.build_request(b));

        let service_sink = request_settings.batch_sink(HttpRetryLogic, svc, cx.acker());
        let inner = BatchSink::from_settings(service_sink, batch, batch_settings);

        Self {
            sink,
            inner,
            slot: None,
        }
    }
}

impl<T, B> Sink for BatchedHttpSink<T, B>
where
    B: Batch,
    B::Output: Clone,
    T: HttpSink<Input = B::Input, Output = B::Output>,
{
    type SinkItem = crate::Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if let Some(item) = self.sink.encode_event(item) {
            if let AsyncSink::NotReady(item) = self.inner.start_send(item)? {
                self.slot = Some(item);
            }
        }

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.inner.poll_complete()
    }
}

#[derive(Clone)]
pub struct HttpService<B = Vec<u8>> {
    inner: InstrumentedService<
        Client<HttpsConnector<HttpConnector<Resolver>>, Vec<u8>>,
        Request<Vec<u8>>,
    >,
    request_builder: Arc<dyn Fn(B) -> hyper::Request<Vec<u8>> + Sync + Send>,
}

impl HttpService<Vec<u8>> {
    pub fn builder(resolver: Resolver) -> HttpServiceBuilder {
        HttpServiceBuilder::new(resolver)
    }

    pub fn new<F>(resolver: Resolver, request_builder: F) -> Self
    where
        F: Fn(Vec<u8>) -> hyper::Request<Vec<u8>> + Sync + Send + 'static,
    {
        Self::builder(resolver).build(request_builder)
    }
}

impl<B> HttpService<B> {
    pub fn new2<F>(
        resolver: Resolver,
        tls_settings: Option<TlsSettings>,
        request_builder: F,
    ) -> HttpService<B>
    where
        F: Fn(B) -> hyper::Request<Vec<u8>> + Sync + Send + 'static,
    {
        let mut http = HttpConnector::new_with_resolver(resolver.clone());
        http.enforce_http(false);

        let mut tls = native_tls::TlsConnector::builder();
        if let Some(settings) = tls_settings {
            tls.use_tls_settings(settings);
        }

        let tls = tls.build().expect("TLS initialization failed");

        let https = HttpsConnector::from((http, tls));
        let client = hyper::Client::builder()
            .executor(DefaultExecutor::current())
            .build(https);

        let inner = Client::with_client(client).instrument(info_span!("http"));
        HttpService {
            inner,
            request_builder: Arc::new(Box::new(request_builder)),
        }
    }
}

/// A builder for `HttpService`s
#[derive(Debug)]
pub struct HttpServiceBuilder {
    resolver: Resolver,
    tls_settings: Option<TlsSettings>,
}

impl HttpServiceBuilder {
    fn new(resolver: Resolver) -> Self {
        Self {
            resolver,
            tls_settings: None,
        }
    }

    /// Build the configured `HttpService`
    pub fn build<F>(self, request_builder: F) -> HttpService<Vec<u8>>
    where
        F: Fn(Vec<u8>) -> hyper::Request<Vec<u8>> + Sync + Send + 'static,
    {
        let mut http = HttpConnector::new_with_resolver(self.resolver.clone());
        http.enforce_http(false);

        let mut tls = native_tls::TlsConnector::builder();
        if let Some(settings) = self.tls_settings {
            tls.use_tls_settings(settings);
        }

        let tls = tls.build().expect("TLS initialization failed");

        let https = HttpsConnector::from((http, tls));
        let client = hyper::Client::builder()
            .executor(DefaultExecutor::current())
            .build(https);

        let inner = Client::with_client(client).instrument(info_span!("http"));
        HttpService {
            inner,
            request_builder: Arc::new(Box::new(request_builder)),
        }
    }

    /// Set the standard TLS settings
    pub fn tls_settings(mut self, settings: TlsSettings) -> Self {
        self.tls_settings = Some(settings);
        self
    }
}

pub fn connector(
    resolver: Resolver,
    tls_settings: TlsSettings,
) -> crate::Result<HttpsConnector<HttpConnector<Resolver>>> {
    let mut http = HttpConnector::new_with_resolver(resolver);
    http.enforce_http(false);

    let mut tls = native_tls::TlsConnector::builder();

    tls.use_tls_settings(tls_settings);

    let https = HttpsConnector::from((http, tls.build()?));

    Ok(https)
}

pub fn https_client(
    resolver: Resolver,
    tls: TlsSettings,
) -> crate::Result<hyper::Client<HttpsConnector<HttpConnector<Resolver>>>> {
    let https = connector(resolver, tls)?;
    Ok(hyper::Client::builder().build(https))
}

impl<B> Service<B> for HttpService<B> {
    type Response = Response;
    type Error = Error;
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, body: B) -> Self::Future {
        let request = (self.request_builder)(body);

        debug!(message = "sending request.");

        let fut = self
            .inner
            .call(request)
            .inspect(|res| {
                debug!(
                    message = "response.",
                    status = &field::display(res.status()),
                    version = &field::debug(res.version()),
                )
            })
            .and_then(|r| {
                let (parts, body) = r.into_parts();
                body.concat2()
                    .map(|b| hyper::Response::from_parts(parts, b.into_bytes()))
            });

        Box::new(fut)
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
}

impl Auth {
    pub fn apply<B>(&self, req: &mut Request<B>) {
        match &self {
            Auth::Basic { user, password } => {
                use headers::HeaderMapExt;
                let auth = headers::Authorization::basic(&user, &password);
                req.headers_mut().typed_insert(auth);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::{Future, Sink, Stream};
    use http::Method;
    use hyper::service::service_fn;
    use hyper::{Body, Response, Server, Uri};
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

    #[test]
    fn util_http_it_makes_http_requests() {
        let rt = crate::test_util::runtime();
        let addr = crate::test_util::next_addr();
        let resolver = Resolver::new(Vec::new(), rt.executor()).unwrap();

        let uri = format!("http://{}:{}/", addr.ip(), addr.port())
            .parse::<Uri>()
            .unwrap();

        let request = b"hello".to_vec();
        let mut service = HttpService::new(resolver, move |body| {
            let mut builder = hyper::Request::builder();
            builder.method(Method::POST);
            builder.uri(uri.clone());
            builder.body(body.into()).unwrap()
        });

        let req = service.call(request);

        let (tx, rx) = futures::sync::mpsc::channel(10);

        let new_service = move || {
            let tx = tx.clone();

            service_fn(move |req: hyper::Request<Body>| -> Box<dyn Future<Item = Response<Body>, Error = String> + Send> {
                let tx = tx.clone();

                Box::new(req.into_body().map_err(|_| "".to_string()).fold::<_, _, Result<_, String>>(vec![], |mut acc, chunk| {
                    acc.extend_from_slice(&chunk);
                    Ok(acc)
                }).and_then(move |v| {
                    let string = String::from_utf8(v).map_err(|_| "Wasn't UTF-8".to_string());
                    tx.send(string).map_err(|_| "Send error".to_string())
                }).and_then(|_| {
                    futures::future::ok(Response::new(Body::from("")))
                }))
            })
        };

        let server = Server::bind(&addr)
            .serve(new_service)
            .map_err(|e| eprintln!("server error: {}", e));

        let mut rt = crate::runtime::Runtime::new().unwrap();

        rt.spawn(server);

        rt.block_on(req).unwrap();

        let _ = rt.shutdown_now();

        let (body, _rest) = rx.into_future().wait().unwrap();
        assert_eq!(body.unwrap().unwrap(), "hello");
    }
}
