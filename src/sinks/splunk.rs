use futures::{future, Future, Sink, Stream};
use hyper::{Body, Client, Request, Uri};
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

pub fn hec(token: String, host: String) -> super::RouterSinkFuture {
    Box::new(future::lazy(|| {
        let (tx, rx) = futures::sync::mpsc::channel(1000);
        let tx = tx.sink_map_err(|e| panic!("{:?}", e));

        let https = HttpsConnector::new(4).expect("TLS initialization failed");

        let client: Client<_, Body> = Client::builder()
            .executor(DefaultExecutor::current())
            .build(https);

        let pump_task = rx
            .for_each(move |record: Record| {
                let body = json!({
                    "event": record.line,
                });
                let body = serde_json::to_vec(&body).unwrap();

                let uri = format!("{}/services/collector/event", host);
                let uri: Uri = uri.parse().unwrap();

                let request = Request::post(uri)
                    .header("Content-Type", "application/json")
                    .header("Authorization", format!("Splunk {}", token))
                    .body(body.into())
                    .unwrap();

                client
                    .request(request)
                    .map_err(|e| panic!("{:?}", e))
                    // .map(|response| println!("{:?}", response))
                    .map(|_| ())
            })
            .map_err(|e| panic!("{:?}", e));
        tokio::spawn(pump_task);

        let x: super::RouterSink = Box::new(tx);

        future::ok(x)
    }))
}
