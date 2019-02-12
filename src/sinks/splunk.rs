use super::util;
use super::util::SinkExt;
use futures::{try_ready, Async, AsyncSink, Future, Poll, Sink};
use hyper::{Request, Uri};
use log::error;
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::codec::{FramedWrite, LinesCodec};
use tokio::net::TcpStream;
use tokio_retry::strategy::ExponentialBackoff;

use crate::record::Record;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TcpSinkConfig {
    pub address: std::net::SocketAddr,
}

#[typetag::serde(name = "splunk_tcp")]
impl crate::topology::config::SinkConfig for TcpSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        Ok((raw_tcp(self.address), tcp_healthcheck(self.address)))
    }
}

struct TcpSink {
    addr: SocketAddr,
    state: TcpSinkState,
    backoff: ExponentialBackoff,
}

enum TcpSinkState {
    Disconnected,
    Connecting(tokio::net::tcp::ConnectFuture),
    Connected(FramedWrite<TcpStream, LinesCodec>),
    Backoff(tokio::timer::Delay),
}

impl TcpSink {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            state: TcpSinkState::Disconnected,
            backoff: Self::fresh_backoff(),
        }
    }

    fn fresh_backoff() -> ExponentialBackoff {
        // TODO: make configurable
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }

    fn poll_connection(&mut self) -> Poll<&mut FramedWrite<TcpStream, LinesCodec>, ()> {
        loop {
            match self.state {
                TcpSinkState::Disconnected => {
                    self.state = TcpSinkState::Connecting(TcpStream::connect(&self.addr));
                }
                TcpSinkState::Backoff(ref mut delay) => match delay.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    // Err can only occur if the tokio runtime has been shutdown or if more than 2^63 timers have been created
                    Err(err) => unreachable!(err),
                    Ok(Async::Ready(())) => {
                        self.state = TcpSinkState::Disconnected;
                    }
                },
                TcpSinkState::Connecting(ref mut connect_future) => match connect_future.poll() {
                    Ok(Async::Ready(socket)) => {
                        self.state =
                            TcpSinkState::Connected(FramedWrite::new(socket, LinesCodec::new()));
                        self.backoff = Self::fresh_backoff();
                    }
                    Ok(Async::NotReady) => {
                        return Ok(Async::NotReady);
                    }
                    Err(err) => {
                        error!("Error connecting to {}: {}", self.addr, err);
                        let delay =
                            tokio::timer::Delay::new(Instant::now() + self.backoff.next().unwrap());
                        self.state = TcpSinkState::Backoff(delay);
                    }
                },
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
        // Stream::forward will immediately poll_complete the sink it's forwarding to,
        // but we don't want to connect before the first record actually comes through.
        if let TcpSinkState::Disconnected = self.state {
            return Ok(Async::Ready(()));
        }

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

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct HecSinkConfig {
    pub token: String,
    pub host: String,
}

#[typetag::serde(name = "splunk_hec")]
impl crate::topology::config::SinkConfig for HecSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        Ok((
            hec(self.token.clone(), self.host.clone()),
            hec_healthcheck(self.token.clone(), self.host.clone()),
        ))
    }
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
