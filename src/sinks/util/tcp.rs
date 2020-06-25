use crate::{
    dns::Resolver,
    emit,
    internal_events::{
        TcpConnectionDisconnected, TcpConnectionEstablished, TcpConnectionFailed,
        TcpConnectionShutdown, TcpEventSent, TcpFlushError,
    },
    sinks::util::{encode_event, encoding::EncodingConfig, Encoding, SinkBuildError, StreamSink},
    sinks::{Healthcheck, RouterSink},
    tls::{MaybeTlsConnector, MaybeTlsSettings, MaybeTlsStream, TlsConfig},
    topology::config::SinkContext,
};
use bytes::Bytes;
use futures01::{
    future, stream::iter_ok, try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::io::{ErrorKind, Read};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio01::{
    codec::{BytesCodec, FramedWrite},
    net::tcp::TcpStream,
    timer::Delay,
};
use tokio_retry::strategy::ExponentialBackoff;

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

    pub fn build(&self, cx: SinkContext) -> crate::Result<(RouterSink, Healthcheck)> {
        let uri = self.address.parse::<http::Uri>()?;

        let host = uri.host().ok_or(SinkBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(SinkBuildError::MissingPort)?;

        let tls = MaybeTlsSettings::from_config(&self.tls, false)?;

        let sink = raw_tcp(host.clone(), port, cx.clone(), self.encoding.clone(), tls);
        let healthcheck = tcp_healthcheck(host, port, cx.resolver());

        Ok((sink, healthcheck))
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
    Connecting(MaybeTlsConnector),
    Connected(TcpOrTlsStream),
    Backoff(Delay),
}

type TcpOrTlsStream = FramedWrite<MaybeTlsStream<TcpStream>, BytesCodec>;

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

    fn fresh_backoff() -> ExponentialBackoff {
        // TODO: make configurable
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }

    fn next_delay(&mut self) -> Delay {
        Delay::new(Instant::now() + self.backoff.next().unwrap())
    }

    fn poll_connection(&mut self) -> Poll<&mut TcpOrTlsStream, ()> {
        loop {
            self.state = match self.state {
                TcpSinkState::Disconnected => {
                    debug!(message = "resolving dns.", host = %self.host);
                    let fut = self.resolver.lookup_ip_01(self.host.clone());

                    TcpSinkState::ResolvingDns(fut)
                }
                TcpSinkState::ResolvingDns(ref mut dns) => match dns.poll() {
                    Ok(Async::Ready(mut ips)) => {
                        if let Some(ip) = ips.next() {
                            let addr = SocketAddr::new(ip, self.port);

                            debug!(message = "connecting", %addr);
                            match self.tls.connect(self.host.clone(), addr) {
                                Ok(connector) => TcpSinkState::Connecting(connector),
                                Err(error) => {
                                    error!(message = "unable to connect", %error);
                                    TcpSinkState::Backoff(self.next_delay())
                                }
                            }
                        } else {
                            error!("DNS resolved but there were no IP addresses.");
                            TcpSinkState::Backoff(self.next_delay())
                        }
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(error) => {
                        error!(message = "unable to resolve dns.", %error);
                        TcpSinkState::Backoff(self.next_delay())
                    }
                },
                TcpSinkState::Backoff(ref mut delay) => match delay.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    // Err can only occur if the tokio runtime has been shutdown or if more than 2^63 timers have been created
                    Err(err) => unreachable!(err),
                    Ok(Async::Ready(())) => {
                        debug!(message = "disconnected.");
                        TcpSinkState::Disconnected
                    }
                },
                TcpSinkState::Connecting(ref mut connect_future) => match connect_future.poll() {
                    Ok(Async::Ready(stream)) => {
                        emit!(TcpConnectionEstablished {
                            peer_addr: stream.peer_addr().ok(),
                        });
                        self.backoff = Self::fresh_backoff();
                        TcpSinkState::Connected(FramedWrite::new(stream, BytesCodec::new()))
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(error) => {
                        emit!(TcpConnectionFailed { error });
                        TcpSinkState::Backoff(self.next_delay())
                    }
                },
                TcpSinkState::Connected(ref mut connection) => return Ok(Async::Ready(connection)),
            };
        }
    }
}

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
                // If this returns `WouldBlock` we know the connection is still
                // valid and the write will most likely succeed.
                match connection.get_mut().read(&mut [0u8; 1]) {
                    Err(error) if error.kind() != ErrorKind::WouldBlock => {
                        emit!(TcpConnectionDisconnected { error });
                        self.state = TcpSinkState::Disconnected;
                        Ok(AsyncSink::NotReady(line))
                    }
                    Ok(0) => {
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
                            Ok(ok) => Ok(ok),
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

pub fn raw_tcp(
    host: String,
    port: u16,
    cx: SinkContext,
    encoding: EncodingConfig<Encoding>,
    tls: MaybeTlsSettings,
) -> RouterSink {
    let tcp = TcpSink::new(host, port, cx.resolver(), tls);
    let sink = StreamSink::new(tcp, cx.acker());
    Box::new(sink.with_flat_map(move |event| iter_ok(encode_event(event, &encoding))))
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: std::io::Error },
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: crate::dns::DnsError },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
}

pub fn tcp_healthcheck(host: String, port: u16, resolver: Resolver) -> Healthcheck {
    // Lazy to avoid immediately connecting
    let check = future::lazy(move || {
        resolver
            .lookup_ip_01(host)
            .map_err(|source| HealthcheckError::DnsError { source }.into())
            .and_then(|mut ip| {
                ip.next()
                    .ok_or_else(|| HealthcheckError::NoAddresses.into())
            })
            .and_then(move |ip| {
                let addr = SocketAddr::new(ip, port);
                TcpStream::connect(&addr)
                    .map(|_| ())
                    .map_err(|source| HealthcheckError::ConnectError { source }.into())
            })
    });

    Box::new(check)
}
