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
use futures::{compat::CompatSink, task::noop_waker_ref, FutureExt, TryFutureExt};
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
    pub encoding: EncodingConfig<Encoding>,
    pub tls: Option<TlsConfig>,
}

impl TcpSinkConfig {
    pub fn new(address: String, encoding: EncodingConfig<Encoding>) -> Self {
        Self {
            address,
            encoding,
            tls: None,
        }
    }

    pub fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let uri = self.address.parse::<http::Uri>()?;

        let host = uri.host().ok_or(SinkBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(SinkBuildError::MissingPort)?;

        let tls = MaybeTlsSettings::from_config(&self.tls, false)?;

        let tcp = TcpSink::new(host, port, cx.resolver(), tls);
        let healthcheck = tcp.healthcheck();

        let encoding = self.encoding.clone();
        let sink = Box::new(
            StreamSink::new(tcp, cx.acker())
                .with_flat_map(move |event| iter_ok(encode_event(event, &encoding))),
        );

        Ok((VectorSink::Futures01Sink(sink), healthcheck))
    }
}

pub struct TcpSink {
    host: String,
    port: u16,
    resolver: Resolver,
    tls: MaybeTlsSettings,
    state: TcpSinkState,
    backoff: ExponentialBackoff,
    span: tracing::Span,
}

enum TcpSinkState {
    Disconnected,
    ResolvingDns(crate::dns::ResolverFuture),
    Connecting(Box<dyn Future<Item = MaybeTlsStream<TcpStream>, Error = TlsError> + Send>),
    Connected(TcpOrTlsStream),
    Backoff(Box<dyn Future<Item = (), Error = ()> + Send>),
}

type TcpOrTlsStream = CompatSink<FramedWrite<MaybeTlsStream<TcpStream>, BytesCodec>, Bytes>;

impl TcpSink {
    pub fn new(host: String, port: u16, resolver: Resolver, tls: MaybeTlsSettings) -> Self {
        let span = info_span!("connection", %host, %port);
        Self {
            host,
            port,
            resolver,
            tls,
            state: TcpSinkState::Disconnected,
            backoff: Self::fresh_backoff(),
            span,
        }
    }

    pub fn healthcheck(&self) -> Healthcheck {
        tcp_healthcheck(
            self.host.clone(),
            self.port,
            self.resolver,
            self.tls.clone(),
        )
        .boxed()
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
                    debug!(message = "resolving DNS.", host = %self.host);
                    let fut = self.resolver.lookup_ip_01(self.host.clone());

                    TcpSinkState::ResolvingDns(fut)
                }
                TcpSinkState::ResolvingDns(ref mut dns) => match dns.poll() {
                    Ok(Async::Ready(mut ips)) => {
                        if let Some(ip) = ips.next() {
                            let addr = SocketAddr::new(ip, self.port);

                            debug!(message = "connecting", %addr);
                            let fut = self.tls.clone().connect(self.host.clone(), addr);
                            let fut = Box::new(fut.boxed().compat());
                            TcpSinkState::Connecting(fut)
                        } else {
                            error!("DNS resolved but there were no IP addresses.");
                            TcpSinkState::Backoff(self.next_delay01())
                        }
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(error) => {
                        error!(message = "unable to resolve DNS.", %error);
                        TcpSinkState::Backoff(self.next_delay01())
                    }
                },
                TcpSinkState::Connecting(ref mut connect_future) => match connect_future.poll() {
                    Ok(Async::Ready(stream)) => {
                        emit!(TcpConnectionEstablished {
                            peer_addr: stream.peer_addr().ok(),
                        });
                        self.backoff = Self::fresh_backoff();
                        let out = FramedWrite::new(stream, BytesCodec::new());
                        TcpSinkState::Connected(CompatSink::new(out))
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(error) => {
                        emit!(TcpConnectionFailed { error });
                        TcpSinkState::Backoff(self.next_delay01())
                    }
                },
                TcpSinkState::Connected(ref mut connection) => return Ok(Async::Ready(connection)),
                TcpSinkState::Backoff(ref mut delay) => match delay.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    // Err can only occur if the tokio runtime has been shutdown or if more than 2^63 timers have been created
                    Err(()) => unreachable!(),
                    Ok(Async::Ready(())) => {
                        debug!(message = "disconnected.");
                        TcpSinkState::Disconnected
                    }
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
                        let byte_size = line.len();
                        match connection.start_send(line) {
                            Ok(AsyncSink::NotReady(line)) => Ok(AsyncSink::NotReady(line)),
                            Err(error) => {
                                error!(message = "connection disconnected.", %error);
                                self.state = TcpSinkState::Disconnected;
                                Ok(AsyncSink::Ready)
                            }
                            Ok(AsyncSink::Ready) => {
                                emit!(TcpEventSent { byte_size });
                                Ok(AsyncSink::Ready)
                            }
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

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: TlsError },
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: crate::dns::DnsError },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
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
        .ok_or_else(|| HealthcheckError::NoAddresses)?;

    let _ = tls
        .connect(host, SocketAddr::new(ip, port))
        .await
        .context(ConnectError)?;

    Ok(())
}
