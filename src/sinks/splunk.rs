use super::util;
use super::util::SinkExt;
use futures::{try_ready, Async, AsyncSink, Future, Poll, Sink};
use hyper::{Request, Uri};
use log::error;
use serde_json::json;
use std::net::SocketAddr;
use tokio::codec::{FramedWrite, LinesCodec};
use tokio::net::TcpStream;

use crate::record::Record;

struct TcpSink {
    addr: SocketAddr,
    state: TcpSinkState,
}

enum TcpSinkState {
    Disconnected,
    Connecting(tokio::net::tcp::ConnectFuture),
    Connected(FramedWrite<TcpStream, LinesCodec>),
}

impl TcpSink {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr: addr,
            state: TcpSinkState::Disconnected,
        }
    }

    fn poll_connection(&mut self) -> Poll<&mut FramedWrite<TcpStream, LinesCodec>, ()> {
        loop {
            match self.state {
                TcpSinkState::Disconnected => {
                    self.state = TcpSinkState::Connecting(TcpStream::connect(&self.addr));
                }
                TcpSinkState::Connecting(ref mut connect_future) => {
                    match connect_future.poll() {
                        Ok(Async::Ready(socket)) => {
                            self.state = TcpSinkState::Connected(FramedWrite::new(
                                socket,
                                LinesCodec::new(),
                            ));
                        }
                        Ok(Async::NotReady) => {
                            return Ok(Async::NotReady);
                        }
                        Err(err) => {
                            // TODO: add backoff
                            error!("Error connecting to {}: {}", self.addr, err);
                            self.state = TcpSinkState::Disconnected;
                        }
                    }
                }
                TcpSinkState::Connected(ref mut connection) => {
                    return Ok(Async::Ready(connection));
                }
            }
        }
    }
}

impl Sink for TcpSink {
    type SinkItem = String;
    type SinkError = ();

    fn start_send(
        &mut self,
        line: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        match self.poll_connection() {
            Ok(Async::Ready(connection)) => match connection.start_send(line) {
                Err(err) => {
                    error!("Error in connection {}: {}", self.addr, err);
                    self.state = TcpSinkState::Disconnected;
                    Ok(AsyncSink::Ready)
                }
                Ok(ok) => Ok(ok),
            },
            Ok(Async::NotReady) => Ok(AsyncSink::NotReady(line)),
            Err(_) => unreachable!(),
        }
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        let connection = try_ready!(self.poll_connection());

        match connection.poll_complete() {
            Err(err) => {
                error!("Error in connection {}: {}", self.addr, err);
                self.state = TcpSinkState::Disconnected;
                Ok(Async::Ready(()))
            }
            Ok(ok) => Ok(ok),
        }
    }
}

pub fn raw_tcp(addr: SocketAddr) -> super::RouterSink {
    Box::new(TcpSink::new(addr).with(|record: Record| Ok(record.line)))
}

pub fn tcp_healthcheck(addr: SocketAddr) -> super::Healthcheck {
    let check = TcpStream::connect(&addr)
        .map(|_| ())
        .map_err(|err| err.to_string());

    Box::new(check)
}

pub fn hec(token: String, host: String) -> super::RouterSink {
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

    Box::new(sink)
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
