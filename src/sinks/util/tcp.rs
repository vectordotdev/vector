use crate::{
    buffers::Acker,
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
    Event,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{stream::BoxStream, task::noop_waker_ref, FutureExt, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{io::AsyncRead, net::TcpStream, time::delay_for};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_util::codec::{BytesCodec, FramedWrite};
use tracing_futures::Instrument;

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

        let encoding = self.encoding.clone();
        let encode_event = Box::new(move |event| encode_event(event, &encoding));

        let sink = TcpSink::new(host, port, tls, cx, encode_event);
        let healthcheck = sink.healthcheck();

        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
    }
}

pub struct TcpSink {
    host: String,
    port: u16,
    tls: MaybeTlsSettings,
    resolver: Resolver,
    acker: Acker,
    encode_event: Box<dyn FnMut(Event) -> Option<Bytes> + Send>,
    state: TcpSinkState,
    backoff: ExponentialBackoff,
    span: tracing::Span,
}

enum TcpSinkState {
    Connected(FramedWrite<MaybeTlsStream<TcpStream>, BytesCodec>),
    Disconnected,
}

impl TcpSink {
    pub fn new(
        host: String,
        port: u16,
        tls: MaybeTlsSettings,
        cx: SinkContext,
        encode_event: Box<dyn FnMut(Event) -> Option<Bytes> + Send>,
    ) -> Self {
        let span = info_span!("connection", %host, %port);
        Self {
            host,
            port,
            tls,
            resolver: cx.resolver(),
            acker: cx.acker(),
            encode_event,
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

    async fn next_delay(&mut self) {
        delay_for(self.backoff.next().unwrap()).await
    }
}

#[async_trait]
impl StreamSink for TcpSink {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        // TODO: use select! for `input.next()` & `???`.
        let mut slot = None;
        loop {
            let event = match slot.take() {
                Some(event) => event,
                None => match input.next().await {
                    Some(event) => match (*self.encode_event)(event) {
                        Some(event) => event,
                        None => continue,
                    },
                    None => break,
                },
            };

            let span = self.span.clone();
            async {
                let stream = loop {
                    match &mut self.state {
                        TcpSinkState::Connected(stream) => break stream,
                        TcpSinkState::Disconnected => {
                            debug!(message = "resolving DNS.", host = %self.host);
                            match self.resolver.lookup_ip(self.host.clone()).await {
                                Ok(mut ips) => match ips.next() {
                                    Some(ip) => {
                                        let addr = SocketAddr::new(ip, self.port);
                                        debug!(message = "connecting", %addr);
                                        let tls = self.tls.clone();
                                        match tls.connect(self.host.clone(), addr).await {
                                            Ok(stream) => {
                                                emit!(TcpConnectionEstablished {
                                                    peer_addr: stream.peer_addr().ok(),
                                                });
                                                let out =
                                                    FramedWrite::new(stream, BytesCodec::new());
                                                self.state = TcpSinkState::Connected(out);
                                                self.backoff = Self::fresh_backoff();
                                            }
                                            Err(error) => {
                                                emit!(TcpConnectionFailed { error });
                                                self.next_delay().await
                                            }
                                        }
                                    }
                                    None => {
                                        error!("DNS resolved but there were no IP addresses.");
                                        self.next_delay().await
                                    }
                                },
                                Err(error) => {
                                    error!(message = "unable to resolve DNS.", %error);
                                    self.next_delay().await
                                }
                            }
                        }
                    }
                };

                // Test if the remote has issued a disconnect by calling read(2)
                // with a 1 sized buffer.
                //
                // This can return a proper disconnect error or `Ok(0)`
                // which means the pipe is broken and we should try to reconnect.
                //
                // If this returns `Poll::Pending` we know the connection is still
                // valid and the write will most likely succeed.
                let inner: &mut MaybeTlsStream<TcpStream> = stream.get_mut();
                let mut cx = Context::from_waker(noop_waker_ref());
                match Pin::new(inner).poll_read(&mut cx, &mut [0u8; 1]) {
                    Poll::Ready(Err(error)) => {
                        emit!(TcpConnectionDisconnected { error });
                        self.state = TcpSinkState::Disconnected;
                        slot = Some(event);
                    }
                    Poll::Ready(Ok(0)) => {
                        // Maybe this is only a sign to close the channel,
                        // in which case we should try to flush our buffers
                        // before disconnecting.
                        // Flush done so we can safely disconnect, or error
                        // in which case we have really been disconnected.
                        let _ = stream.close().await;
                        emit!(TcpConnectionShutdown {});
                        self.state = TcpSinkState::Disconnected;
                        slot = Some(event);
                    }
                    _ => {
                        let byte_size = event.len();
                        match stream.send(event).await {
                            Ok(()) => emit!(TcpEventSent { byte_size }),
                            Err(error) => {
                                error!(message = "connection disconnected.", %error);
                                self.state = TcpSinkState::Disconnected;
                            }
                        }
                        self.acker.ack(1);
                    }
                }
            }
            .instrument(span)
            .await;
        }

        if let TcpSinkState::Connected(stream) = &mut self.state {
            async {
                if let Err(error) = stream.close().await {
                    emit!(TcpFlushError { error });
                }
            }
            .instrument(self.span.clone())
            .await
        }

        Ok(())
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
