use super::{encode_event, encoding::EncodingConfig, Encoding, SinkBuildError, StreamSinkOld};
use crate::{
    config::SinkContext,
    dns::{Resolver, ResolverFuture},
    internal_events::UdpSendIncomplete,
    sinks::{Healthcheck, VectorSink},
};
use bytes::Bytes;
use futures::{future, FutureExt, TryFutureExt};
use futures01::{stream::iter_ok, Async, AsyncSink, Future, Poll, Sink, StartSend};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::Duration;
use tokio::time::{delay_for, Delay};
use tokio_retry::strategy::ExponentialBackoff;

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

    pub fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let uri = self.address.parse::<http::Uri>()?;

        let host = uri.host().ok_or(SinkBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(SinkBuildError::MissingPort)?;

        let encoding = self.encoding.clone();
        let sink = UdpSink::new(host, port, cx.resolver());
        let sink = StreamSinkOld::new(sink, cx.acker())
            .with_flat_map(move |event| iter_ok(encode_event(event, &encoding)));
        let healthcheck = udp_healthcheck();

        Ok((VectorSink::Futures01Sink(Box::new(sink)), healthcheck))
    }
}

fn udp_healthcheck() -> Healthcheck {
    future::ok(()).boxed()
}

pub struct UdpSink {
    host: String,
    port: u16,
    resolver: Resolver,
    state: State,
    span: tracing::Span,
    backoff: ExponentialBackoff,
}

enum State {
    Initializing,
    ResolvingDns(ResolverFuture),
    ResolvedDns(SocketAddr),
    Backoff(Box<dyn Future<Item = (), Error = ()> + Send>),
    Connected(UdpSocket),
}

impl UdpSink {
    pub fn new(host: String, port: u16, resolver: Resolver) -> Self {
        let span = info_span!("connection", %host, %port);
        Self {
            host,
            port,
            resolver,
            state: State::Initializing,
            span,
            backoff: Self::fresh_backoff(),
        }
    }

    fn fresh_backoff() -> ExponentialBackoff {
        // TODO: make configurable
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }

    fn next_delay(&mut self) -> Delay {
        delay_for(self.backoff.next().unwrap())
    }

    fn next_delay01(&mut self) -> Box<dyn Future<Item = (), Error = ()> + Send> {
        let delay = self.next_delay();
        Box::new(async move { Ok(delay.await) }.boxed().compat())
    }

    fn poll_inner(&mut self) -> Result<Async<&mut UdpSocket>, ()> {
        loop {
            self.state = match self.state {
                State::Initializing => {
                    debug!(message = "resolving DNS", host = %self.host);
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
                            State::Backoff(self.next_delay01())
                        }
                    },
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(error) => {
                        error!(message = "unable to resolve DNS", host = %self.host, %error);
                        State::Backoff(self.next_delay01())
                    }
                },
                State::ResolvedDns(addr) => {
                    let bind_address = find_bind_address(&addr);
                    match UdpSocket::bind(bind_address) {
                        Ok(socket) => match socket.connect(addr) {
                            Ok(()) => State::Connected(socket),
                            Err(error) => {
                                error!(message = "unable to connect UDP socket", %addr, %error);
                                State::Backoff(self.next_delay01())
                            }
                        },
                        Err(error) => {
                            error!(message = "unable to bind local address", addr = %bind_address, %error);
                            State::Backoff(self.next_delay01())
                        }
                    }
                }
                State::Connected(ref mut socket) => return Ok(Async::Ready(socket)),
                State::Backoff(ref mut delay) => match delay.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Ok(Async::Ready(())) => State::Initializing,
                    Err(_) => unreachable!(),
                },
            }
        }
    }
}

fn find_bind_address(remote_addr: &SocketAddr) -> SocketAddr {
    match remote_addr {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}

impl Sink for UdpSink {
    type SinkItem = Bytes;
    type SinkError = ();

    fn start_send(&mut self, line: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let span = self.span.clone();
        let _enter = span.enter();

        match self.poll_inner() {
            Ok(Async::Ready(socket)) => {
                debug!(
                    message = "sending event.",
                    bytes = %line.len()
                );
                match socket.send(&line) {
                    Err(error) => {
                        self.state = State::Backoff(self.next_delay01());
                        error!(message = "send failed", %error);
                        Ok(AsyncSink::NotReady(line))
                    }
                    Ok(sent) => {
                        if sent != line.len() {
                            emit!(UdpSendIncomplete {
                                data_size: line.len(),
                                sent,
                            });
                        }
                        Ok(AsyncSink::Ready)
                    }
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
