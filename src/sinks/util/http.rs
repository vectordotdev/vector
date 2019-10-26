use super::{
    retries::RetryLogic,
    tls::{TlsConnectorExt, TlsSettings},
};
use bytes::Bytes;
use futures::{Future, Poll, Stream};
use http::{Request, StatusCode};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use native_tls::TlsConnector;
use std::borrow::Cow;
use std::sync::Arc;
use tokio::executor::DefaultExecutor;
use tower::Service;
use tower_hyper::client::Client;
use tracing::field;
use tracing_tower::{InstrumentableService, InstrumentedService};

pub type RequestBuilder = Box<dyn Fn(Vec<u8>) -> hyper::Request<Vec<u8>> + Sync + Send>;
pub type Response = hyper::Response<Bytes>;
pub type Error = hyper::Error;

#[derive(Clone)]
pub struct HttpService {
    inner: InstrumentedService<Client<HttpsConnector<HttpConnector>, Vec<u8>>, Request<Vec<u8>>>,
    request_builder: Arc<RequestBuilder>,
}

impl HttpService {
    pub fn builder() -> HttpServiceBuilder {
        HttpServiceBuilder::new()
    }

    pub fn new<F>(request_builder: F) -> Self
    where
        F: Fn(Vec<u8>) -> hyper::Request<Vec<u8>> + Sync + Send + 'static,
    {
        Self::builder().build(request_builder)
    }
}

/// A builder for `HttpService`s
#[derive(Default)]
pub struct HttpServiceBuilder {
    threads: usize,
    tls_settings: Option<TlsSettings>,
}

impl HttpServiceBuilder {
    fn new() -> Self {
        Self {
            threads: 4,
            ..Default::default()
        }
    }

    /// Build the configured `HttpService`
    pub fn build<F>(self, request_builder: F) -> HttpService
    where
        F: Fn(Vec<u8>) -> hyper::Request<Vec<u8>> + Sync + Send + 'static,
    {
        let mut http = HttpConnector::new(self.threads);
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

    /// Set the number of threads used by the `HttpService`
    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    /// Set the standard TLS settings
    pub fn tls_settings(mut self, settings: TlsSettings) -> Self {
        self.tls_settings = Some(settings);
        self
    }
}

pub fn https_client(
    tls: TlsSettings,
) -> crate::Result<hyper::Client<HttpsConnector<HttpConnector>>> {
    let mut http = HttpConnector::new(1);
    http.enforce_http(false);
    let tls = TlsConnector::builder().use_tls_settings(tls).build()?;
    let https = HttpsConnector::from((http, tls));
    Ok(hyper::Client::builder().build(https))
}

impl Service<Vec<u8>> for HttpService {
    type Response = Response;
    type Error = Error;
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, body: Vec<u8>) -> Self::Future {
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

    fn should_retry_response(&self, response: &Self::Response) -> Option<Cow<str>> {
        let status = response.status();

        match status {
            StatusCode::TOO_MANY_REQUESTS => Some("Too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => None,
            _ if status.is_server_error() => {
                Some(format!("{}: {}", status, String::from_utf8_lossy(response.body())).into())
            }
            _ => None,
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

        assert!(logic.should_retry_response(&response_429).is_some());
        assert!(logic.should_retry_response(&response_500).is_some());
        assert!(logic.should_retry_response(&response_400).is_none());
        assert!(logic.should_retry_response(&response_501).is_none());
    }

    #[test]
    fn util_http_it_makes_http_requests() {
        let addr = crate::test_util::next_addr();
        let uri = format!("http://{}:{}/", addr.ip(), addr.port())
            .parse::<Uri>()
            .unwrap();

        let request = b"hello".to_vec();
        let mut service = HttpService::new(move |body| {
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

        rt.shutdown_now();

        let (body, _rest) = rx.into_future().wait().unwrap();
        assert_eq!(body.unwrap().unwrap(), "hello");
    }
}
