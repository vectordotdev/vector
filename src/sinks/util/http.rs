use futures::{try_ready, Async, AsyncSink, Future, Sink};
use hyper::{
    client::{HttpConnector, ResponseFuture},
    Body, Client, Request,
};
use hyper_tls::HttpsConnector;
use log::error;
use tokio::executor::DefaultExecutor;


pub struct HttpSink {
    client: Client<HttpsConnector<HttpConnector>, Body>,
    in_flight_request: Option<ResponseFuture>,
}

impl HttpSink {
    pub fn new() -> Self {
        let https = HttpsConnector::new(4).expect("TLS initialization failed");
        let client: Client<_, Body> = Client::builder()
            .executor(DefaultExecutor::current())
            .build(https);

        Self {
            client,
            in_flight_request: None,
        }
    }
}

impl Sink for HttpSink {
    type SinkItem = Request<Body>;
    type SinkError = ();

    fn start_send(
        &mut self,
        request: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        if self.in_flight_request.is_some() {
            self.poll_complete()?;
            if self.in_flight_request.is_some() {
                return Ok(AsyncSink::NotReady(request));
            }
        }

        let request = self.client.request(request);

        self.in_flight_request = Some(request);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        loop {
            if let Some(ref mut in_flight_request) = self.in_flight_request {
                let _response =
                    try_ready!(in_flight_request.poll().map_err(|e| error!("err: {}", e)));

                // TODO: retry on errors

                self.in_flight_request = None;
            } else {
                return Ok(Async::Ready(()));
            }
        }
    }
}

#[cfg(test)]
mod test {
    use hyper::{Request, Body, Response, Server, Uri};
    use super::HttpSink;
    use futures::{Sink,Future,Stream};
    use hyper::service::service_fn;

    #[test]
    fn it_makes_http_requests() {
        let addr = crate::test_util::next_addr();
        let uri = format!("http://{}:{}/", addr.ip(), addr.port()).parse::<Uri>().unwrap();


        let request = Request::post(uri).body(Body::from("hello")).unwrap();
        let sink = HttpSink::new();

        let req = sink.send(request);


        let (tx, rx) = futures::sync::mpsc::channel(10);

        let new_service = move || {
            let tx = tx.clone();

            service_fn(move |req: Request<Body>| -> Box<dyn Future<Item = Response<Body>, Error = String> + Send> {
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
