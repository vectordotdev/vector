use crate::{
    dns::Resolver,
    sinks::util::{
        encode_event,
        encoding::{EncodingConfig, EncodingConfiguration},
        Encoding, SinkExt,
    },
    sinks::{Healthcheck, RouterSink},
    tls::{TlsConfig, TlsConnectorExt, TlsSettings},
    topology::config::SinkContext,
};
use bytes::Bytes;
use futures01::{
    future, stream::iter_ok, try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::{
    codec::{BytesCodec, FramedWrite},
    net::tcp::{ConnectFuture, TcpStream},
    timer::Delay,
};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_tls::{Connect as TlsConnect, TlsConnector, TlsStream};
use tracing::field;

#[derive(Debug, Snafu)]
enum TcpBuildError {
    #[snafu(display("Must specify both TLS key_file and crt_file"))]
    MissingCrtKeyFile,
    #[snafu(display("Could not build TLS connector: {}", source))]
    TlsBuildError { source: native_tls::Error },
    #[snafu(display("Could not set TCP TLS identity: {}", source))]
    TlsIdentityError { source: native_tls::Error },
    #[snafu(display("Could not export identity to DER: {}", source))]
    DerExportError { source: openssl::error::ErrorStack },
    #[snafu(display("Missing host in address field"))]
    MissingHost,
    #[snafu(display("Missing port in address field"))]
    MissingPort,
}

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
        self.encoding.validate()?;
        let uri = self.address.parse::<http::Uri>()?;

        let host = uri.host().ok_or(TcpBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(TcpBuildError::MissingPort)?;

        let tls = TlsSettings::from_config(&self.tls, false)?;

        let sink = raw_tcp(host.clone(), port, cx.clone(), self.encoding.clone(), tls);
        let healthcheck = tcp_healthcheck(host, port, cx.resolver());

        Ok((sink, healthcheck))
    }
}

pub struct TcpSink {
    host: String,
    port: u16,
    resolver: Resolver,
    tls: Option<TlsSettings>,
    state: TcpSinkState,
    backoff: ExponentialBackoff,
    span: tracing::Span,
}

enum TcpSinkState {
    Disconnected,
    ResolvingDns(crate::dns::ResolverFuture),
    Connecting(ConnectFuture),
    TlsConnecting(TlsConnect<TcpStream>),
    Connected(TcpOrTlsStream),
    Backoff(Delay),
}

type TcpOrTlsStream = MaybeTlsStream<
    FramedWrite<TcpStream, BytesCodec>,
    FramedWrite<TlsStream<TcpStream>, BytesCodec>,
>;

impl TcpSink {
    pub fn new(host: String, port: u16, resolver: Resolver, tls: Option<TlsSettings>) -> Self {
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
                    let fut = self.resolver.lookup_ip(&self.host);

                    TcpSinkState::ResolvingDns(fut)
                }
                TcpSinkState::ResolvingDns(ref mut dns) => match dns.poll() {
                    Ok(Async::Ready(mut ips)) => {
                        if let Some(ip) = ips.next() {
                            let addr = SocketAddr::new(ip, self.port);

                            debug!(message = "connecting", %addr);
                            TcpSinkState::Connecting(TcpStream::connect(&addr))
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
                    Ok(Async::Ready(socket)) => {
                        debug!(message = "connected");
                        self.backoff = Self::fresh_backoff();
                        match self.tls {
                            Some(ref tls) => match native_tls::TlsConnector::builder()
                                .use_tls_settings(tls.clone())
                                .build()
                                .context(TlsBuildError)
                            {
                                Ok(connector) => TcpSinkState::TlsConnecting(
                                    TlsConnector::from(connector).connect(&self.host, socket),
                                ),
                                Err(err) => {
                                    error!(message = "unable to establish TLS connection.", error = %err);
                                    TcpSinkState::Backoff(self.next_delay())
                                }
                            },
                            None => TcpSinkState::Connected(MaybeTlsStream::Raw(FramedWrite::new(
                                socket,
                                BytesCodec::new(),
                            ))),
                        }
                    }
                    Ok(Async::NotReady) => {
                        return Ok(Async::NotReady);
                    }
                    Err(error) => {
                        error!(message = "unable to connect.", %error);
                        TcpSinkState::Backoff(self.next_delay())
                    }
                },
                TcpSinkState::TlsConnecting(ref mut connect_future) => {
                    match connect_future.poll() {
                        Ok(Async::Ready(socket)) => {
                            debug!(message = "negotiated TLS.");
                            self.backoff = Self::fresh_backoff();
                            TcpSinkState::Connected(MaybeTlsStream::Tls(FramedWrite::new(
                                socket,
                                BytesCodec::new(),
                            )))
                        }
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(error) => {
                            error!(message = "unable to negotiate TLS.", %error);
                            TcpSinkState::Backoff(self.next_delay())
                        }
                    }
                }
                TcpSinkState::Connected(ref mut connection) => {
                    return Ok(Async::Ready(connection));
                }
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
                debug!(
                    message = "sending event.",
                    bytes = &field::display(line.len())
                );
                match connection.start_send(line) {
                    Err(error) => {
                        error!(message = "connection disconnected.", %error);
                        self.state = TcpSinkState::Disconnected;
                        Ok(AsyncSink::Ready)
                    }
                    Ok(ok) => Ok(ok),
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
                error!(message = "unable to flush connection.", %error);
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
    tls: Option<TlsSettings>,
) -> RouterSink {
    Box::new(
        TcpSink::new(host, port, cx.resolver(), tls)
            .stream_ack(cx.acker())
            .with_flat_map(move |event| iter_ok(encode_event(event, &encoding))),
    )
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
            .lookup_ip(host)
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

enum MaybeTlsStream<R, T> {
    Raw(R),
    Tls(T),
}

impl<R, T, I, E> Sink for MaybeTlsStream<R, T>
where
    R: Sink<SinkItem = I, SinkError = E>,
    T: Sink<SinkItem = I, SinkError = E>,
{
    type SinkItem = I;
    type SinkError = E;

    fn start_send(&mut self, item: I) -> futures01::StartSend<I, E> {
        match self {
            MaybeTlsStream::Raw(r) => r.start_send(item),
            MaybeTlsStream::Tls(t) => t.start_send(item),
        }
    }

    fn poll_complete(&mut self) -> futures01::Poll<(), E> {
        match self {
            MaybeTlsStream::Raw(r) => r.poll_complete(),
            MaybeTlsStream::Tls(t) => t.poll_complete(),
        }
    }
}
