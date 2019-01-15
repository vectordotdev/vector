use super::util;
use super::util::SinkExt;
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

pub fn tcp_healthcheck(addr: SocketAddr) -> super::Healthcheck {
    let check = TcpStream::connect(&addr)
        .map(|_| ())
        .map_err(|err| err.to_string());

    Box::new(check)
}

pub fn hec(token: String, host: String) -> super::RouterSinkFuture {
    let sink = util::http::HttpSink::new()
        .with(move |body: Vec<u8>| {
            let uri = format!("{}/services/collector/event", host);
            let uri: Uri = uri.parse().unwrap();

            let request = Request::post(uri)
                .header("Content-Type", "application/json")
                .header("Content-Encoding", "gzip")
                .header("Authorization", format!("Splunk {}", token))
                .body(body.into())
                .unwrap();

            Ok(request)
        })
        .size_buffered(2 * 1024 * 1024, true)
        .with(move |record: Record| {
            let mut body = json!({
                "event": record.line,
                "fields": record.custom,
            });
            if let Some(host) = record.host {
                body["host"] = json!(host);
            }
            let body = serde_json::to_vec(&body).unwrap();
            Ok(body)
        });

    let sink: super::RouterSink = Box::new(sink);
    Box::new(future::ok(sink))
}

pub fn hec_healthcheck(token: String, host: String) -> super::Healthcheck {
    use hyper::{Body, Client, Request};
    use hyper_tls::HttpsConnector;

    let uri = format!("{}/services/collector/health/1.0", host);
    let uri: Uri = uri.parse().unwrap();

    let request = Request::get(uri)
        .header("Authorization", format!("Splunk {}", token))
        .body(Body::empty())
        .unwrap();

    let https = HttpsConnector::new(4).expect("TLS initialization failed");
    let client = Client::builder().build(https);

    let healthcheck = client
        .request(request)
        .map_err(|err| err.to_string())
        .and_then(|response| {
            use hyper::StatusCode;

            match response.status() {
                StatusCode::OK => Ok(()),
                StatusCode::BAD_REQUEST => Err("Invalid HEC token".to_string()),
                StatusCode::SERVICE_UNAVAILABLE => {
                    Err("HEC is unhealthy, queues are full".to_string())
                }
                other => Err(format!("Unexpected status: {}", other)),
            }
        });

    Box::new(healthcheck)
}
