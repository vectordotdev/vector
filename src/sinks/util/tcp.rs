use crate::{
    config::SinkContext,
    dns::Resolver,
    emit,
    internal_events::{
        ConnectionOpen, OpenGauge, OpenTokenDyn, TcpConnectionDisconnected,
        TcpConnectionEstablished, TcpConnectionFailed, TcpConnectionShutdown, TcpEventSent,
        TcpFlushError,
    },
    sinks::util::{SinkBuildError, StreamSinkOld},
    sinks::{Healthcheck, VectorSink},
    tls::{MaybeTlsSettings, MaybeTlsStream, TlsConfig, TlsError},
    Event,
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
struct TcpConnector {
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
    #[snafu(display("Send error: {}", source))]
    SendError { source: tokio::io::Error },
}

impl TcpSinkConfig {
    pub fn new(address: String) -> Self {
        Self { address, tls: None }
    }

    fn build_connector(&self, cx: SinkContext) -> crate::Result<TcpConnector> {
        let uri = self.address.parse::<http::Uri>()?;

        let host = uri.host().ok_or(SinkBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(SinkBuildError::MissingPort)?;

        let tls = MaybeTlsSettings::from_config(&self.tls, false)?;

        let connector = TcpConnector::new(host, port, cx.resolver(), tls);

        Ok(connector)
    }

    pub fn build<F>(
        &self,
        cx: SinkContext,
        encode_event: F,
    ) -> crate::Result<(VectorSink, Healthcheck)>
    where
        F: Fn(Event) -> Option<Bytes> + Send + 'static,
    {
        let connector = self.build_connector(cx.clone())?;
        let healthcheck = connector.healthcheck();
        let sink: TcpSink = connector.into();
        let sink = StreamSinkOld::new(sink, cx.acker())
            .with_flat_map(move |event| iter_ok(encode_event(event)));

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

    fn connect(&self) -> BoxFuture<'static, Result<TcpOrTlsStream, TcpError>> {
        let host = self.host.clone();
        let port = self.port;
        let resolver = self.resolver;
        let tls = self.tls.clone();

        async move {
            let ip = resolver
                .lookup_ip(host.clone())
                .await
                .context(DnsError)?
                .next()
                .ok_or(TcpError::NoAddresses)?;

            let addr = SocketAddr::new(ip, port);
            let stream = tls.connect(host, addr).await.context(ConnectError)?;
            Ok(FramedWrite::new(stream, BytesCodec::new()))
        }
        .boxed()
    }

    fn healthcheck(&self) -> BoxFuture<'static, crate::Result<()>> {
        self.connect().map_ok(|_| ()).map_err(Into::into).boxed()
    }
}

impl Into<TcpSink> for TcpConnector {
    fn into(self) -> TcpSink {
        TcpSink::new(self.host, self.port, self.resolver, self.tls)
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
    Connected(TcpOrTlsStream01, OpenTokenDyn),
    Backoff(Box<dyn Future<Item = (), Error = ()> + Send>),
}

type TcpOrTlsStream = FramedWrite<MaybeTlsStream<TcpStream>, BytesCodec>;
type TcpOrTlsStream01 = CompatSink<TcpOrTlsStream, Bytes>;

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

    fn poll_connection(&mut self) -> Poll01<&mut TcpOrTlsStream01, ()> {
        loop {
            self.state = match self.state {
                TcpSinkState::Disconnected => {
                    TcpSinkState::Connecting(Box::new(self.connector.connect().compat()))
                }
                TcpSinkState::Connecting(ref mut connect_future) => match connect_future.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Ok(Async::Ready(mut connection)) => {
                        emit!(TcpConnectionEstablished {
                            peer_addr: connection.get_mut().peer_addr().ok(),
                        });
                        self.backoff = Self::fresh_backoff();
                        TcpSinkState::Connected(
                            CompatSink::new(connection),
                            OpenGauge::new()
                                .open(Box::new(|count| emit!(ConnectionOpen { count }))),
                        )
                    }
                    Err(error) => {
                        emit!(TcpConnectionFailed { error });
                        TcpSinkState::Backoff(self.next_delay01())
                    }
                },
                TcpSinkState::Connected(ref mut connection, _) => {
                    return Ok(Async::Ready(connection))
                }
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

        loop {
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
                                Ok(Async::NotReady) => return Ok(AsyncSink::NotReady(line)),
                            }
                        }
                        _ => {
                            let byte_size = line.len();
                            return match connection.start_send(line) {
                                Ok(AsyncSink::NotReady(line)) => Ok(AsyncSink::NotReady(line)),
                                Err(error) => {
                                    error!(message = "connection disconnected.", %error);
                                    self.state = TcpSinkState::Disconnected;
                                    return Ok(AsyncSink::Ready);
                                }
                                Ok(AsyncSink::Ready) => {
                                    emit!(TcpEventSent { byte_size });
                                    Ok(AsyncSink::Ready)
                                }
                            };
                        }
                    }
                }
                Ok(Async::NotReady) => return Ok(AsyncSink::NotReady(line)),
                Err(_) => unreachable!(),
            }
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_util::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn healthcheck() {
        trace_init();

        let addr = next_addr();
        let resolver = crate::dns::Resolver;

        let _listener = TcpListener::bind(&addr).await.unwrap();

        let healthcheck =
            TcpConnector::new(addr.ip().to_string(), addr.port(), resolver, None.into())
                .healthcheck();

        assert!(healthcheck.await.is_ok());

        let bad_addr = next_addr();
        let bad_healthcheck = TcpConnector::new(
            bad_addr.ip().to_string(),
            bad_addr.port(),
            resolver,
            None.into(),
        )
        .healthcheck();

        assert!(bad_healthcheck.await.is_err());
    }
}
