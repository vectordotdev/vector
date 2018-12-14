use futures::{try_ready, Async, AsyncSink, Future, Sink};
use hyper::{
    client::{HttpConnector, ResponseFuture},
    Body, Client, Request,
};
use hyper_tls::HttpsConnector;
use log::error;
use tokio::executor::DefaultExecutor;


// TODO: test this against a fake server
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
