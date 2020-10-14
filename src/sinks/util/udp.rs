use super::{encode_event, encoding::EncodingConfig, Encoding, SinkBuildError, StreamSinkOld};
use crate::{
    config::SinkContext,
    dns::Resolver,
    internal_events::UdpSendIncomplete,
    sinks::{Healthcheck, VectorSink},
};
use bytes::Bytes;
use futures::{future::BoxFuture, FutureExt, TryFutureExt};
use futures01::{stream::iter_ok, Async, AsyncSink, Future, Poll as Poll01, Sink, StartSend};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{delay_for, Delay};
use tokio_retry::strategy::ExponentialBackoff;

#[derive(Debug, Snafu)]
pub enum UdpError {
    #[snafu(display("Failed to create UDP listener socket, error = {:?}.", source))]
    BindError { source: std::io::Error },
    #[snafu(display("Send error: {}", source))]
    SendError { source: std::io::Error },
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: std::io::Error },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: crate::dns::DnsError },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UdpSinkConfig {
    pub address: String,
}

impl UdpSinkConfig {
    pub fn new(address: String) -> Self {
        Self { address }
    }

    fn build_connector(&self, cx: SinkContext) -> crate::Result<(UdpConnector, Healthcheck)> {
        let uri = self.address.parse::<http::Uri>()?;

        let host = uri.host().ok_or(SinkBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(SinkBuildError::MissingPort)?;

        let connector = UdpConnector::new(host, port, cx.resolver());
        let healthcheck = connector.healthcheck();

        Ok((connector, healthcheck))
    }

    pub fn build_service(&self, cx: SinkContext) -> crate::Result<(UdpService, Healthcheck)> {
        let (connector, healthcheck) = self.build_connector(cx)?;
        Ok((connector.into(), healthcheck))
    }

    pub fn build(
        &self,
        cx: SinkContext,
        encoding: EncodingConfig<Encoding>,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let (connector, healthcheck) = self.build_connector(cx.clone())?;
        let sink: UdpSink = connector.into();
        let sink = StreamSinkOld::new(sink, cx.acker())
            .with_flat_map(move |event| iter_ok(encode_event(event, &encoding)));

        Ok((VectorSink::Futures01Sink(Box::new(sink)), healthcheck))
    }
}

#[derive(Clone)]
struct UdpConnector {
    host: String,
    port: u16,
    resolver: Resolver,
}

impl UdpConnector {
    fn new(host: String, port: u16, resolver: Resolver) -> Self {
        Self {
            host,
            port,
            resolver,
        }
    }

    fn connect(&self) -> BoxFuture<'static, Result<UdpSocket, UdpError>> {
        let host = self.host.clone();
        let port = self.port;
        let resolver = self.resolver;

        async move {
            let ip = resolver
                .lookup_ip(host.clone())
                .await
                .context(DnsError)?
                .next()
                .ok_or(UdpError::NoAddresses)?;

            let addr = SocketAddr::new(ip, port);
            let bind_address = find_bind_address(&addr);

            let socket = UdpSocket::bind(bind_address).context(BindError)?;
            socket.connect(addr).context(ConnectError)?;

            Ok(socket)
        }
        .boxed()
    }

    fn healthcheck(&self) -> BoxFuture<'static, crate::Result<()>> {
        self.connect().map_ok(|_| ()).map_err(|e| e.into()).boxed()
    }
}

impl Into<UdpSink> for UdpConnector {
    fn into(self) -> UdpSink {
        UdpSink::new(self.host, self.port, self.resolver)
    }
}

impl Into<UdpService> for UdpConnector {
    fn into(self) -> UdpService {
        UdpService { connector: self }
    }
}

pub struct UdpService {
    connector: UdpConnector,
}

impl tower::Service<Bytes> for UdpService {
    type Response = ();
    type Error = UdpError;
    type Future = BoxFuture<'static, Result<(), Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Bytes) -> Self::Future {
        let connector = self.connector.clone();
        async move {
            let socket = connector.connect().await?;
            socket.send(&msg).context(SendError)?;
            Ok(())
        }
        .boxed()
    }
}

pub struct UdpSink {
    connector: UdpConnector,
    state: State,
    span: tracing::Span,
    backoff: ExponentialBackoff,
}

enum State {
    Initializing,
    Connecting(Box<dyn Future<Item = UdpSocket, Error = UdpError> + Send>),
    Connected(UdpSocket),
    Backoff(Box<dyn Future<Item = (), Error = ()> + Send>),
}

impl UdpSink {
    pub fn new(host: String, port: u16, resolver: Resolver) -> Self {
        let span = info_span!("connection", %host, %port);
        let connector = UdpConnector {
            host,
            port,
            resolver,
        };
        Self {
            connector,
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

    fn poll_socket(&mut self) -> Poll01<&mut UdpSocket, ()> {
        loop {
            self.state = match self.state {
                State::Initializing => {
                    State::Connecting(Box::new(self.connector.connect().compat()))
                }
                State::Connecting(ref mut fut) => match fut.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Ok(Async::Ready(socket)) => State::Connected(socket),
                    Err(error) => {
                        error!(message = "unable to connect UDP socket", %error);
                        State::Backoff(self.next_delay01())
                    }
                },
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

impl Sink for UdpSink {
    type SinkItem = Bytes;
    type SinkError = ();

    fn start_send(&mut self, line: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let span = self.span.clone();
        let _enter = span.enter();

        match self.poll_socket() {
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

    fn poll_complete(&mut self) -> Poll01<(), Self::SinkError> {
        Ok(Async::Ready(()))
    }
}

fn find_bind_address(remote_addr: &SocketAddr) -> SocketAddr {
    match remote_addr {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}
