use super::{encode_event, encoding::EncodingConfig, Encoding, SinkBuildError, StreamSink};
use crate::{
    buffers::Acker,
    config::SinkContext,
    dns::Resolver,
    sinks::{Healthcheck, VectorSink},
    Event,
};
use async_trait::async_trait;
use futures::{future, stream::BoxStream, FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};
use tokio::{net::UdpSocket, time::delay_for};
use tokio_retry::strategy::ExponentialBackoff;
use tracing::field;
use tracing_futures::Instrument;

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
        let sink = UdpSink::new(host, port, cx, encoding);
        let healthcheck = udp_healthcheck();

        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
    }
}

fn udp_healthcheck() -> Healthcheck {
    future::ok(()).boxed()
}

pub struct UdpSink {
    host: String,
    port: u16,
    resolver: Resolver,
    acker: Acker,
    encoding: EncodingConfig<Encoding>,
    state: UdpSinkState,
    backoff: ExponentialBackoff,
    span: tracing::Span,
}

enum UdpSinkState {
    Connected(UdpSocket),
    Disconnected,
}

impl UdpSink {
    pub fn new(
        host: String,
        port: u16,
        cx: SinkContext,
        encoding: EncodingConfig<Encoding>,
    ) -> Self {
        let span = info_span!("connection", %host, %port);
        Self {
            host,
            port,
            resolver: cx.resolver(),
            acker: cx.acker(),
            encoding,
            state: UdpSinkState::Disconnected,
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

    async fn next_delay(&mut self) {
        delay_for(self.backoff.next().unwrap()).await
    }
}

#[async_trait]
impl StreamSink for UdpSink {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        // TODO: use select! for `input.next()` & `socket.send`.
        while let Some(event) = input.next().await {
            let event = match encode_event(event, &self.encoding) {
                Some(event) => event,
                None => continue,
            };

            let span = self.span.clone();
            async {
                let socket = loop {
                    match &mut self.state {
                        UdpSinkState::Connected(stream) => break stream,
                        UdpSinkState::Disconnected => {
                            debug!(message = "resolving DNS", host = %self.host);
                            match self.resolver.lookup_ip(self.host.clone()).await {
                                Ok(mut addrs) => match addrs.next() {
                                    Some(addr) => {
                                        let addr = SocketAddr::new(addr, self.port);
                                        debug!(message = "resolved address", %addr);
                                        let bind_address = find_bind_address(&addr);
                                        match UdpSocket::bind(bind_address).await {
                                            Ok(socket) => match socket.connect(addr).await {
                                                Ok(()) => {
                                                    self.state = UdpSinkState::Connected(socket);
                                                    self.backoff = Self::fresh_backoff()
                                                },
                                                Err(error) => {
                                                    error!(message = "unable to connect UDP socket", %addr, %error);
                                                    self.next_delay().await
                                                }
                                            },
                                            Err(error) => {
                                                error!(message = "unable to bind local address", addr = %bind_address, %error);
                                                self.next_delay().await
                                            }
                                        }
                                    }
                                    None => {
                                        error!(message = "DNS resolved no addresses", host = %self.host);
                                        self.next_delay().await
                                    }
                                },
                                Err(error) => {
                                    error!(message = "unable to resolve DNS", host = %self.host, %error);
                                    self.next_delay().await
                                }
                            }
                        }
                    }
                };

                debug!(
                    message = "sending event.",
                    bytes = &field::display(event.len())
                );
                if let Err(error) = socket.send(&event).await {
                    error!(message = "send failed", %error);
                    self.state = UdpSinkState::Disconnected;
                }

                self.acker.ack(1);
            }
            .instrument(span)
            .await
        }

        Ok(())
    }
}

fn find_bind_address(remote_addr: &SocketAddr) -> SocketAddr {
    match remote_addr {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}
