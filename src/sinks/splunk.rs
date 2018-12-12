use futures::{future, try_ready, Async, AsyncSink, Future, Sink};
use hyper::{
    client::{HttpConnector, ResponseFuture},
    Body, Client, Request, Uri,
};
use hyper_tls::HttpsConnector;
use log::error;
use serde_json::json;
use std::net::SocketAddr;
use tokio::codec::{FramedWrite, LinesCodec};
use tokio::executor::DefaultExecutor;
use tokio::net::TcpStream;

use crate::record::Record;

pub fn raw_tcp(addr: SocketAddr) -> super::RouterSinkFuture {
    // lazy so that we don't actually try to connect until the future is polled
    Box::new(future::lazy(move || {
        TcpStream::connect(&addr)
            .map(|socket| -> super::RouterSink {
                Box::new(
                    FramedWrite::new(socket, LinesCodec::new())
                        .sink_map_err(|e| error!("splunk sink error: {:?}", e))
                        .with(|record: Record| Ok(record.line)),
                )
            })
            .map_err(|e| error!("error opening splunk sink: {:?}", e))
    }))
}

struct Hec {
    client: Client<HttpsConnector<HttpConnector>, Body>,
    in_flight_request: Option<ResponseFuture>,

    token: String,
    host: String,
}

impl Hec {
    pub fn new(token: String, host: String) -> Self {
        let https = HttpsConnector::new(4).expect("TLS initialization failed");
        let client: Client<_, Body> = Client::builder()
            .executor(DefaultExecutor::current())
            .build(https);

        Self {
            client,
            token,
            host,
            in_flight_request: None,
        }
    }
}

pub fn hec(token: String, host: String) -> super::RouterSinkFuture {
    let sink: super::RouterSink = Box::new(Hec::new(token, host));
    Box::new(future::ok(sink))
}

impl Sink for Hec {
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(
        &mut self,
        record: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        if self.in_flight_request.is_some() {
            return Ok(AsyncSink::NotReady(record));
        } else {
            let body = json!({
                "event": record.line,
            });
            let body = serde_json::to_vec(&body).unwrap();

            let uri = format!("{}/services/collector/event", self.host);
            let uri: Uri = uri.parse().unwrap();

            let request = Request::post(uri)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Splunk {}", self.token))
                .body(body.into())
                .unwrap();

            let request = self.client.request(request);

            self.in_flight_request = Some(request);

            Ok(AsyncSink::Ready)
        }
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
