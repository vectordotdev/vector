use bytes::Bytes;
use codec::BytesDelimitedCodec;
use futures::{future, try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::codec::FramedWrite;
use tokio::net::TcpStream;
use tokio_retry::strategy::ExponentialBackoff;

use crate::record::Record;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TcpSinkConfig {
    pub address: std::net::SocketAddr,
}

#[typetag::serde(name = "tcp")]
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
    Connected(FramedWrite<TcpStream, BytesDelimitedCodec>),
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

    fn poll_connection(&mut self) -> Poll<&mut FramedWrite<TcpStream, BytesDelimitedCodec>, ()> {
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
                        self.state = TcpSinkState::Connected(FramedWrite::new(
                            socket,
                            BytesDelimitedCodec::new(b'\n'),
                        ));
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
    type SinkItem = Bytes;
    type SinkError = ();

    fn start_send(&mut self, line: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
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
    Box::new(TcpSink::new(addr).with(|record: Record| Ok(record.raw)))
}

pub fn tcp_healthcheck(addr: SocketAddr) -> super::Healthcheck {
    // Lazy to avoid immediately connecting
    let check = future::lazy(move || {
        TcpStream::connect(&addr)
            .map(|_| ())
            .map_err(|err| err.to_string())
    });

    Box::new(check)
}
