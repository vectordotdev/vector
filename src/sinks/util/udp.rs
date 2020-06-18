use super::{encode_event, encoding::EncodingConfig, Encoding, SinkBuildError, StreamSink};
use crate::{
    dns::{Resolver, ResolverFuture},
    sinks::{Healthcheck, RouterSink},
    topology::config::SinkContext,
};
use bytes::Bytes;
use futures01::{future, stream::iter_ok, Async, AsyncSink, Future, Poll, Sink, StartSend};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};
use tokio01::timer::Delay;
use tokio_retry::strategy::ExponentialBackoff;
use tracing::field;

#[derive(Debug, Snafu)]
pub enum UdpBuildError {
    #[snafu(display("failed to create UDP listener socket, error = {:?}", source))]
    SocketBind { source: io::Error },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UdpSinkConfig {
    pub address: String,
    pub encoding: EncodingConfig<Encoding>,
}

impl UdpSinkConfig {
    pub fn new(address: String, encoding: EncodingConfig<Encoding>) -> Self {
        Self { address, encoding }
    }

    pub fn build(&self, cx: SinkContext) -> crate::Result<(RouterSink, Healthcheck)> {
        let uri = self.address.parse::<http::Uri>()?;

        let host = uri.host().ok_or(SinkBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(SinkBuildError::MissingPort)?;

        let sink = raw_udp(host, port, self.encoding.clone(), cx)?;
        let healthcheck = udp_healthcheck();

        Ok((sink, healthcheck))
    }
}

pub fn raw_udp(
    host: String,
    port: u16,
    encoding: EncodingConfig<Encoding>,
    cx: SinkContext,
) -> Result<RouterSink, UdpBuildError> {
    let sink = UdpSink::new(host, port, cx.resolver())?;
    let sink = StreamSink::new(sink, cx.acker());
    Ok(Box::new(sink.with_flat_map(move |event| {
        iter_ok(encode_event(event, &encoding))
    })))
}

fn udp_healthcheck() -> Healthcheck {
    Box::new(future::ok(()))
}

pub struct UdpSink {
    host: String,
    port: u16,
    resolver: Resolver,
    state: State,
    span: tracing::Span,
    backoff: ExponentialBackoff,
    socket: UdpSocket,
}

enum State {
    Initializing,
    ResolvingDns(ResolverFuture),
    ResolvedDns(SocketAddr),
    Backoff(Delay),
}

impl UdpSink {
    pub fn new(host: String, port: u16, resolver: Resolver) -> Result<Self, UdpBuildError> {
        let from = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let span = info_span!("connection", %host, %port);
        Ok(Self {
            host,
            port,
            resolver,
            state: State::Initializing,
            span,
            backoff: Self::fresh_backoff(),
            socket: UdpSocket::bind(&from).context(SocketBind)?,
        })
    }

    fn fresh_backoff() -> ExponentialBackoff {
        // TODO: make configurable
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }

    fn next_delay(&mut self) -> Delay {
        Delay::new(Instant::now() + self.backoff.next().unwrap())
    }

    fn poll_inner(&mut self) -> Result<Async<SocketAddr>, ()> {
        loop {
            self.state = match self.state {
                State::Initializing => {
                    debug!(message = "resolving dns", host = %self.host);
                    State::ResolvingDns(self.resolver.lookup_ip_01(self.host.clone()))
                }
                State::ResolvingDns(ref mut dns) => match dns.poll() {
                    Ok(Async::Ready(mut addrs)) => match addrs.next() {
                        Some(addr) => {
                            let addr = SocketAddr::new(addr, self.port);
                            debug!(message = "resolved address", %addr);
                            State::ResolvedDns(addr)
                        }
                        None => {
                            error!(message = "DNS resolved no addresses", host = %self.host);
                            State::Backoff(self.next_delay())
                        }
                    },
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(error) => {
                        error!(message = "unable to resolve DNS", host = %self.host, %error);
                        State::Backoff(self.next_delay())
                    }
                },
                State::ResolvedDns(addr) => return Ok(Async::Ready(addr)),
                State::Backoff(ref mut delay) => match delay.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    // Err can only occur if the tokio runtime has been shutdown or if more than 2^63 timers have been created
                    Err(err) => unreachable!(err),
                    Ok(Async::Ready(())) => State::Initializing,
                },
            }
        }
    }
}

impl Sink for UdpSink {
    type SinkItem = Bytes;
    type SinkError = ();

    fn start_send(&mut self, line: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let span = self.span.clone();
        let _enter = span.enter();

        match self.poll_inner() {
            Ok(Async::Ready(address)) => {
                debug!(
                    message = "sending event.",
                    bytes = &field::display(line.len())
                );
                match self.socket.send_to(&line, address) {
                    Err(error) => {
                        error!(message = "send failed", %error);
                        Err(())
                    }
                    Ok(_) => Ok(AsyncSink::Ready),
                }
            }
            Ok(Async::NotReady) => Ok(AsyncSink::NotReady(line)),
            Err(_) => unreachable!(),
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(Async::Ready(()))
    }
}
