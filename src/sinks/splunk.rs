use super::util;
use futures::{future, Future, Sink};
use hyper::{Request, Uri};
use log::error;
use serde_json::json;
use std::net::SocketAddr;
use tokio::codec::{FramedWrite, LinesCodec};
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
    let sink = util::SizeBuffered::new(
        util::HttpSink::new().with(move |body: Vec<u8>| {
            let uri = format!("{}/services/collector/event", host);
            let uri: Uri = uri.parse().unwrap();

            let request = Request::post(uri)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Splunk {}", token))
                .body(body.into())
                .unwrap();

            Ok(request)
        }),
        2 * 1024 * 1024,
    )
    .with(move |record: Record| {
        let body = json!({
            "event": record.line,
            "fields": record.custom,
        });
        let body = serde_json::to_vec(&body).unwrap();
        Ok(body)
    });

    let sink: super::RouterSink = Box::new(sink);
    Box::new(future::ok(sink))
}
