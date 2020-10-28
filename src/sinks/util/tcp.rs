use crate::{
    buffers::Acker,
    config::SinkContext,
    dns::Resolver,
    internal_events::{
        ConnectionOpen, OpenGauge, SocketEventSent, SocketMode, TcpSocketConnectionEstablished,
        TcpSocketConnectionFailed, TcpSocketConnectionShutdown, TcpSocketError,
    },
    sinks::{
        util::{
            socket_events_counter::{BytesSink, EventsCounter, ShutdownCheck},
            SinkBuildError, StreamSink,
        },
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, MaybeTlsStream, TlsConfig, TlsError},
    Event,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{future, stream::BoxStream, task::noop_waker_ref, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    io::ErrorKind,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{io::AsyncRead, net::TcpStream, time::delay_for};
use tokio_retry::strategy::ExponentialBackoff;

#[derive(Debug, Snafu)]
enum TcpError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: TlsError },
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: crate::dns::DnsError },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
    #[snafu(display("Send error: {}", source))]
    SendError { source: tokio::io::Error },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct TcpSinkConfig {
    pub address: String,
    pub tls: Option<TlsConfig>,
}

impl TcpSinkConfig {
    pub fn new(address: String, tls: Option<TlsConfig>) -> Self {
        Self { address, tls }
    }

    pub fn build(
        &self,
        cx: SinkContext,
        encode_event: impl Fn(Event) -> Option<Bytes> + Send + Sync + 'static,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let uri = self.address.parse::<http::Uri>()?;
        let host = uri.host().ok_or(SinkBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(SinkBuildError::MissingPort)?;
        let tls = MaybeTlsSettings::from_config(&self.tls, false)?;

        let connector = TcpConnector::new(host, port, cx.resolver(), tls);
        let sink = TcpSink::new(connector.clone(), cx.acker(), encode_event);

        Ok((
            VectorSink::Stream(Box::new(sink)),
            Box::pin(async move { connector.healthcheck().await }),
        ))
    }
}

#[derive(Clone)]
struct TcpConnector {
    host: String,
    port: u16,
    resolver: Resolver,
    tls: MaybeTlsSettings,
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

    fn fresh_backoff() -> ExponentialBackoff {
        // TODO: make configurable
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }

    async fn connect(&self) -> Result<MaybeTlsStream<TcpStream>, TcpError> {
        let ip = self
            .resolver
            .lookup_ip(self.host.clone())
            .await
            .context(DnsError)?
            .next()
            .ok_or(TcpError::NoAddresses)?;

        let addr = SocketAddr::new(ip, self.port);
        self.tls
            .connect(&self.host, &addr)
            .await
            .context(ConnectError)
    }

    async fn connect_backoff(&self) -> MaybeTlsStream<TcpStream> {
        let mut backoff = Self::fresh_backoff();
        loop {
            match self.connect().await {
                Ok(socket) => {
                    emit!(TcpSocketConnectionEstablished {
                        peer_addr: socket.peer_addr().ok(),
                    });
                    return socket;
                }
                Err(error) => {
                    emit!(TcpSocketConnectionFailed { error });
                    delay_for(backoff.next().unwrap()).await;
                }
            }
        }
    }

    async fn healthcheck(&self) -> crate::Result<()> {
        self.connect().await.map(|_| ()).map_err(Into::into)
    }
}

struct TcpSink {
    connector: TcpConnector,
    events_counter: Arc<EventsCounter>,
}

impl TcpSink {
    fn new(
        connector: TcpConnector,
        acker: Acker,
        encode_event: impl Fn(Event) -> Option<Bytes> + Send + Sync + 'static,
    ) -> Self {
        let on_success = |byte_size| {
            emit!(SocketEventSent {
                byte_size,
                mode: SocketMode::Tcp,
            })
        };
        Self {
            connector,
            events_counter: Arc::new(EventsCounter::new(acker, encode_event, on_success)),
        }
    }

    async fn connect(&self) -> BytesSink<MaybeTlsStream<TcpStream>> {
        let stream = self.connector.connect_backoff().await;
        BytesSink::new(
            stream,
            Self::shutdown_check,
            Arc::clone(&self.events_counter),
        )
    }

    fn shutdown_check(stream: &mut MaybeTlsStream<TcpStream>) -> ShutdownCheck {
        // Test if the remote has issued a disconnect by calling read(2)
        // with a 1 sized buffer.
        //
        // This can return a proper disconnect error or `Ok(0)`
        // which means the pipe is broken and we should try to reconnect.
        //
        // If this returns `Poll::Pending` we know the connection is still
        // valid and the write will most likely succeed.
        let mut cx = Context::from_waker(noop_waker_ref());
        match Pin::new(stream).poll_read(&mut cx, &mut [0u8; 1]) {
            Poll::Ready(Err(error)) => ShutdownCheck::Error(error),
            Poll::Ready(Ok(0)) => {
                // Maybe this is only a sign to close the channel,
                // in which case we should try to flush our buffers
                // before disconnecting.
                ShutdownCheck::Close("ShutdownCheck::Close")
            }
            _ => ShutdownCheck::Alive,
        }
    }
}

#[async_trait]
impl StreamSink for TcpSink {
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // We need [Peekable](https://docs.rs/futures/0.3.6/futures/stream/struct.Peekable.html) for initiating
        // connection only when we have something to send.
        // It would be cool encode events before `peekable()` but [SendAll](https://docs.rs/futures-util/0.3.6/src/futures_util/sink/send_all.rs.html)
        // have buffered item, so if our connection fail we will completely lost one item.
        // As result `EventsCounter` was added for calculating encoded events and ack only consumed items on failed send.
        let mut input = Some(input.peekable());
        while Pin::new(input.as_mut().unwrap()).peek().await.is_some() {
            let events_counter = Arc::clone(&self.events_counter);
            let encode_event = move |event| future::ready(events_counter.encode_event(event));
            let mut stream = input.take().unwrap().filter_map(encode_event);

            let mut sink = self.connect().await;
            let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));
            if let Err(error) = sink.send_all(&mut stream).await {
                if error.kind() == ErrorKind::Other && error.to_string() == "ShutdownCheck::Close" {
                    emit!(TcpSocketConnectionShutdown {});
                } else {
                    emit!(TcpSocketError { error });
                }
            }

            input.replace(stream.into_inner());
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_util::{next_addr, trace_init};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn healthcheck() {
        trace_init();

        let addr = next_addr();
        let _listener = TcpListener::bind(&addr).await.unwrap();
        let good = TcpConnector::new(addr.ip().to_string(), addr.port(), Resolver, None.into());
        assert!(good.healthcheck().await.is_ok());

        let addr = next_addr();
        let bad = TcpConnector::new(addr.ip().to_string(), addr.port(), Resolver, None.into());
        assert!(bad.healthcheck().await.is_err());
    }
}
