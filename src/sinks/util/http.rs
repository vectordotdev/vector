use super::retries::RetryLogic;
use bytes::Bytes;
use futures::{Future, Poll, Stream};
use http::StatusCode;
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use std::sync::Arc;
use tokio::executor::DefaultExecutor;
use tower::Service;
use tower_hyper::client::Client;
use tracing::field;
use tracing_tower_http::InstrumentedHttpService;

pub type RequestBuilder = Box<dyn Fn(Vec<u8>) -> hyper::Request<Vec<u8>> + Sync + Send>;
pub type Response = hyper::Response<Bytes>;

#[derive(Clone)]
pub struct HttpService {
    inner: InstrumentedHttpService<Client<HttpsConnector<HttpConnector>, Vec<u8>>>,
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
pub struct HttpServiceBuilder {
    threads: usize,
    verify_certificate: bool,
}

impl HttpServiceBuilder {
    fn new() -> Self {
        Self {
            threads: 4,
            verify_certificate: true,
        }
    }

    /// Build the configured `HttpService`
    pub fn build<F>(&self, request_builder: F) -> HttpService
    where
        F: Fn(Vec<u8>) -> hyper::Request<Vec<u8>> + Sync + Send + 'static,
    {
        let mut http = HttpConnector::new(self.threads);
        http.enforce_http(false);
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(!self.verify_certificate)
            .build()
            .expect("TLS initialization failed");
        let https = HttpsConnector::from((http, tls));
        let client = hyper::Client::builder()
            .executor(DefaultExecutor::current())
            .build(https);
        let inner = InstrumentedHttpService::new(Client::with_client(client));
        HttpService {
            inner,
            request_builder: Arc::new(Box::new(request_builder)),
        }
    }

    /// Set the number of threads used by the `HttpService`
    pub fn threads(&mut self, threads: usize) -> &mut Self {
        self.threads = threads;
        self
    }

    /// Verify the remote server's certificate
    pub fn verify_certificate(&mut self, verify: bool) -> &mut Self {
        self.verify_certificate = verify;
        self
    }
}

impl Service<Vec<u8>> for HttpService {
    type Response = Response;
    type Error = hyper::Error;
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

    fn should_retry_response(&self, response: &Self::Response) -> bool {
        let status = response.status();

        (status.is_server_error() && status != StatusCode::NOT_IMPLEMENTED)
            || status == StatusCode::TOO_MANY_REQUESTS
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

        assert!(logic.should_retry_response(&response_429));
        assert!(logic.should_retry_response(&response_500));
        assert!(!logic.should_retry_response(&response_400));
        assert!(!logic.should_retry_response(&response_501));
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

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.spawn(server);

        rt.block_on(req).unwrap();

        rt.shutdown_now();

        let (body, _rest) = rx.into_future().wait().unwrap();
        assert_eq!(body.unwrap().unwrap(), "hello");
    }
}
