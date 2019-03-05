use super::retries::{FixedRetryPolicy, RetryLogic};
use futures::{Poll, Sink};
use http::{header::HeaderValue, HeaderMap, HttpTryFrom, Method, Uri};
use hyper::{
    client::{HttpConnector, ResponseFuture},
    Body, Client,
};
use hyper_tls::HttpsConnector;
use std::time::Duration;
use tokio::executor::DefaultExecutor;
use tower_retry::Retry;
use tower_service::Service;
use tower_timeout::Timeout;

#[derive(Clone)]
pub struct HttpSink {
    client: Client<HttpsConnector<HttpConnector>, Body>,
}

// We need our Request to be Clone to support retries. Hyper's Request type is not Clone because it
// supports streaming/chunked bodies and the extensions type map. We don't use either of those
// features, so we can do something a bit simpler than libraries like tower-hyper.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub uri: Uri,
    pub headers: HeaderMap<HeaderValue>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn post(uri: Uri, body: Vec<u8>) -> Self {
        Request {
            method: Method::POST,
            uri,
            headers: Default::default(),
            body,
        }
    }

    pub fn header(&mut self, name: &'static str, value: impl AsRef<str>) -> &mut Self {
        let value = HeaderValue::try_from(value.as_ref()).unwrap();
        self.headers.append(name, value);
        self
    }
}

impl From<Request> for hyper::Request<Body> {
    fn from(req: Request) -> Self {
        let mut builder = hyper::Request::builder();
        builder.method(req.method);
        builder.uri(req.uri);

        for (k, v) in req.headers.iter() {
            builder.header(k, v.as_ref());
        }

        builder.body(req.body.into()).unwrap()
    }
}

impl HttpSink {
    pub fn new() -> impl Sink<SinkItem = Request, SinkError = ()> {
        let https = HttpsConnector::new(4).expect("TLS initialization failed");
        let client: Client<_, Body> = Client::builder()
            .executor(DefaultExecutor::current())
            .build(https);

        let policy = FixedRetryPolicy::new(5, Duration::from_secs(1), HttpRetryLogic);

        let inner = Self { client };
        let timeout = Timeout::new(inner, Duration::from_secs(10));
        let service = Retry::new(policy, timeout);

        super::ServiceSink::new(service)
    }
}

impl Service<Request> for HttpSink {
    type Response = hyper::Response<Body>;
    type Error = hyper::Error;
    type Future = ResponseFuture;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, request: Request) -> Self::Future {
        self.client.request(request.into())
    }
}

#[derive(Clone)]
struct HttpRetryLogic;

impl RetryLogic for HttpRetryLogic {
    type Error = hyper::Error;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_connect() || error.is_closed()
    }
}

#[cfg(test)]
mod test {
    use super::{HttpSink, Request};
    use futures::{Future, Sink, Stream};
    use hyper::service::service_fn;
    use hyper::{Body, Response, Server, Uri};

    #[test]
    fn it_makes_http_requests() {
        let addr = crate::test_util::next_addr();
        let uri = format!("http://{}:{}/", addr.ip(), addr.port())
            .parse::<Uri>()
            .unwrap();

        let request = Request::post(uri, String::from("hello").into_bytes());
        let sink = HttpSink::new();

        let req = sink.send(request);

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
