use crate::{
    config::SinkContext,
    dns::Resolver,
    emit,
    internal_events::{
        TcpConnectionDisconnected, TcpConnectionEstablished, TcpConnectionFailed,
        TcpConnectionShutdown, TcpEventSent, TcpFlushError,
    },
    sinks::util::{encode_event, encoding::EncodingConfig, Encoding, SinkBuildError, StreamSink},
    sinks::{Healthcheck, VectorSink},
    tls::{MaybeTlsSettings, MaybeTlsStream, TlsConfig, TlsError},
};
use bytes::Bytes;
use futures::{
    compat::CompatSink, future::BoxFuture, task::noop_waker_ref, FutureExt, TryFutureExt,
};
use futures01::{
    stream::iter_ok, try_ready, Async, AsyncSink, Future, Poll as Poll01, Sink, StartSend,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::AsyncRead,
    net::TcpStream,
    time::{delay_for, Delay},
};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_util::codec::{BytesCodec, FramedWrite};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct TcpSinkConfig {
    pub address: String,
    pub tls: Option<TlsConfig>,
}

#[derive(Clone)]
pub struct TcpConnector {
    host: String,
    port: u16,
    resolver: Resolver,
    tls: MaybeTlsSettings,
}

#[derive(Debug, Snafu)]
pub enum TcpError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: TlsError },
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: crate::dns::DnsError },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
}

impl TcpSinkConfig {
    pub fn new(address: String) -> Self {
        Self { address, tls: None }
    }

    pub fn prepare(&self, cx: SinkContext) -> crate::Result<(TcpConnector, Healthcheck)> {
        let uri = self.address.parse::<http::Uri>()?;

        let host = uri.host().ok_or(SinkBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(SinkBuildError::MissingPort)?;

        let tls = MaybeTlsSettings::from_config(&self.tls, false)?;

        let tcp = TcpConnector::new(host, port, cx.resolver(), tls);
        let healthcheck = Box::new(tcp.healthcheck().compat());

        Ok((tcp, healthcheck))
    }

    pub fn build(
        &self,
        cx: SinkContext,
        encoding: EncodingConfig<Encoding>,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let (tcp, healthcheck) = self.prepare(cx.clone())?;
        let sink = StreamSink::new(tcp.into_sink(), cx.acker())
            .with_flat_map(move |event| iter_ok(encode_event(event, &encoding)));

        Ok((VectorSink::Futures01Sink(Box::new(sink)), healthcheck))
    }
}

impl TcpConnector {
    fn new(host: String, port: u16, resolver: Resolver, tls: MaybeTlsSettings) -> Self {
        Self {
            host,
            port,
            resolver,
            tls,
        }
    }

    pub fn connect(&self) -> BoxFuture<'static, Result<TcpOrTlsStream, TcpError>> {
        let host = self.host.clone();
        let resolver = self.resolver.clone();
        let port = self.port;
        let tls = self.tls.clone();

        async move {
            debug!(message = "resolving DNS.", host = %host);
            let ip = resolver
                .lookup_ip(host.clone())
                .await
                .context(DnsError)?
                .next()
                .ok_or(TcpError::NoAddresses)?;

            let addr = SocketAddr::new(ip, port);
            debug!(message = "connecting", %addr);
            let stream = tls.connect(host, addr).await.context(ConnectError)?;
            Ok(CompatSink::new(FramedWrite::new(stream, BytesCodec::new())))
        }
        .boxed()
    }

    pub fn into_sink(self) -> TcpSink {
        TcpSink::new(self.host, self.port, self.resolver, self.tls)
    }

    fn healthcheck(&self) -> BoxFuture<'static, crate::Result<()>> {
        tcp_healthcheck(
            self.host.clone(),
            self.port,
            self.resolver,
            self.tls.clone(),
        )
        .boxed()
    }
}

pub struct TcpSink {
    connector: TcpConnector,
    state: TcpSinkState,
    backoff: ExponentialBackoff,
    span: tracing::Span,
}

enum TcpSinkState {
    Disconnected,
    Connecting(Box<dyn Future<Item = TcpOrTlsStream, Error = TcpError> + Send>),
    Connected(TcpOrTlsStream),
    Backoff(Box<dyn Future<Item = (), Error = ()> + Send>),
}

type TcpOrTlsStream = CompatSink<FramedWrite<MaybeTlsStream<TcpStream>, BytesCodec>, Bytes>;

impl TcpSink {
    pub fn new(host: String, port: u16, resolver: Resolver, tls: MaybeTlsSettings) -> Self {
        let span = info_span!("connection", %host, %port);
        let connector = TcpConnector {
            host,
            port,
            resolver,
            tls,
        };
        Self {
            connector,
            state: TcpSinkState::Disconnected,
            backoff: Self::fresh_backoff(),
            span,
        }
    }

    pub fn healthcheck(&self) -> BoxFuture<'static, crate::Result<()>> {
        self.connector.healthcheck()
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

    fn poll_connection(&mut self) -> Poll01<&mut TcpOrTlsStream, ()> {
        loop {
            self.state = match self.state {
                TcpSinkState::Disconnected => {
                    TcpSinkState::Connecting(Box::new(self.connector.connect().compat()))
                }
                TcpSinkState::Connecting(ref mut connect_future) => match connect_future.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Ok(Async::Ready(stream)) => {
                        emit!(TcpConnectionEstablished {
                            peer_addr: stream.get_mut().get_mut().peer_addr().ok(),
                        });
                        self.backoff = Self::fresh_backoff();
                        TcpSinkState::Connected(stream)
                    }
                    Err(error) => {
                        emit!(TcpConnectionFailed { error });
                        TcpSinkState::Backoff(self.next_delay01())
                    }
                },
                TcpSinkState::Connected(ref mut connection) => return Ok(Async::Ready(connection)),
                TcpSinkState::Backoff(ref mut delay) => match delay.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Ok(Async::Ready(())) => TcpSinkState::Disconnected,
                    // Err can only occur if the tokio runtime has been shutdown or if more than 2^63 timers have been created
                    Err(()) => unreachable!(),
                },
            };
        }
    }
}

// New Sink trait implemented in PR#3188: https://github.com/timberio/vector/pull/3188#discussion_r463843208
impl Sink for TcpSink {
    type SinkItem = Bytes;
    type SinkError = ();

    fn start_send(&mut self, line: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let span = self.span.clone();
        let _enter = span.enter();

        match self.poll_connection() {
            Ok(Async::Ready(connection)) => {
                // Test if the remote has issued a disconnect by calling read(2)
                // with a 1 sized buffer.
                //
                // This can return a proper disconnect error or `Ok(0)`
                // which means the pipe is broken and we should try to reconnect.
                //
                // If this returns `Poll::Pending` we know the connection is still
                // valid and the write will most likely succeed.

                let stream: &mut MaybeTlsStream<TcpStream> = connection.get_mut().get_mut();
                let mut cx = Context::from_waker(noop_waker_ref());
                match Pin::new(stream).poll_read(&mut cx, &mut [0u8; 1]) {
                    Poll::Ready(Err(error)) => {
                        emit!(TcpConnectionDisconnected { error });
                        self.state = TcpSinkState::Disconnected;
                        Ok(AsyncSink::NotReady(line))
                    }
                    Poll::Ready(Ok(0)) => {
                        // Maybe this is only a sign to close the channel,
                        // in which case we should try to flush our buffers
                        // before disconnecting.
                        match connection.poll_complete() {
                            // Flush done so we can safely disconnect, or
                            // error in which case we have really been
                            // disconnected.
                            Ok(Async::Ready(())) | Err(_) => {
                                emit!(TcpConnectionShutdown {});
                                self.state = TcpSinkState::Disconnected;
                            }
                            Ok(Async::NotReady) => (),
                        }

                        Ok(AsyncSink::NotReady(line))
                    }
                    _ => {
                        emit!(TcpEventSent {
                            byte_size: line.len()
                        });
                        match connection.start_send(line) {
                            Err(error) => {
                                error!(message = "connection disconnected.", %error);
                                self.state = TcpSinkState::Disconnected;
                                Ok(AsyncSink::Ready)
                            }
                            Ok(res) => Ok(res),
                        }
                    }
                }
            }
            Ok(Async::NotReady) => Ok(AsyncSink::NotReady(line)),
            Err(_) => unreachable!(),
        }
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        // Stream::forward will immediately poll_complete the sink it's forwarding to,
        // but we don't want to connect before the first event actually comes through.
        if let TcpSinkState::Disconnected = self.state {
            return Ok(Async::Ready(()));
        }

        let span = self.span.clone();
        let _enter = span.enter();

        let connection = try_ready!(self.poll_connection());

        match connection.poll_complete() {
            Err(error) => {
                emit!(TcpFlushError { error });
                self.state = TcpSinkState::Disconnected;
                Ok(Async::Ready(()))
            }
            Ok(ok) => Ok(ok),
        }
    }
}

pub async fn tcp_healthcheck(
    host: String,
    port: u16,
    resolver: Resolver,
    tls: MaybeTlsSettings,
) -> crate::Result<()> {
    let ip = resolver
        .lookup_ip(host.clone())
        .await
        .context(DnsError)?
        .next()
        .ok_or_else(|| TcpError::NoAddresses)?;

    let _ = tls
        .connect(host, SocketAddr::new(ip, port))
        .await
        .context(ConnectError)?;

    Ok(())
}
