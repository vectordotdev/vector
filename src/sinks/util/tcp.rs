use std::{
    io::ErrorKind,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use futures::{stream::BoxStream, task::noop_waker_ref, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio::{
    io::{AsyncRead, ReadBuf},
    net::TcpStream,
    time::sleep,
};
use vector_core::{buffers::Acker, ByteSizeOf};

use crate::{
    config::SinkContext,
    dns,
    event::Event,
    internal_events::{
        ConnectionOpen, OpenGauge, SocketMode, TcpSocketConnectionEstablished,
        TcpSocketConnectionFailed, TcpSocketConnectionShutdown, TcpSocketError,
    },
    sink::VecSinkExt,
    sinks::{
        util::{
            retries::ExponentialBackoff,
            socket_bytes_sink::{BytesSink, ShutdownCheck},
            EncodedEvent, SinkBuildError, StreamSink,
        },
        Healthcheck, VectorSink,
    },
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, MaybeTlsStream, TlsConfig, TlsError},
};

#[derive(Debug, Snafu)]
enum TcpError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: TlsError },
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: dns::DnsError },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
    #[snafu(display("Send error: {}", source))]
    SendError { source: tokio::io::Error },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct TcpSinkConfig {
    address: String,
    keepalive: Option<TcpKeepaliveConfig>,
    tls: Option<TlsConfig>,
    send_buffer_bytes: Option<usize>,
}

impl TcpSinkConfig {
    pub const fn new(
        address: String,
        keepalive: Option<TcpKeepaliveConfig>,
        tls: Option<TlsConfig>,
        send_buffer_bytes: Option<usize>,
    ) -> Self {
        Self {
            address,
            keepalive,
            tls,
            send_buffer_bytes,
        }
    }

    pub const fn from_address(address: String) -> Self {
        Self {
            address,
            keepalive: None,
            tls: None,
            send_buffer_bytes: None,
        }
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
        let connector = TcpConnector::new(host, port, self.keepalive, tls, self.send_buffer_bytes);
        let sink = TcpSink::new(connector.clone(), cx.acker(), encode_event);

        Ok((
            VectorSink::from_event_streamsink(sink),
            Box::pin(async move { connector.healthcheck().await }),
        ))
    }
}

#[derive(Clone)]
struct TcpConnector {
    host: String,
    port: u16,
    keepalive: Option<TcpKeepaliveConfig>,
    tls: MaybeTlsSettings,
    send_buffer_bytes: Option<usize>,
}

impl TcpConnector {
    const fn new(
        host: String,
        port: u16,
        keepalive: Option<TcpKeepaliveConfig>,
        tls: MaybeTlsSettings,
        send_buffer_bytes: Option<usize>,
    ) -> Self {
        Self {
            host,
            port,
            keepalive,
            tls,
            send_buffer_bytes,
        }
    }

    #[cfg(test)]
    fn from_host_port(host: String, port: u16) -> Self {
        Self::new(host, port, None, None.into(), None)
    }

    const fn fresh_backoff() -> ExponentialBackoff {
        // TODO: make configurable
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }

    async fn connect(&self) -> Result<MaybeTlsStream<TcpStream>, TcpError> {
        let ip = dns::Resolver
            .lookup_ip(self.host.clone())
            .await
            .context(DnsSnafu)?
            .next()
            .ok_or(TcpError::NoAddresses)?;

        let addr = SocketAddr::new(ip, self.port);
        self.tls
            .connect(&self.host, &addr)
            .await
            .context(ConnectSnafu)
            .map(|mut maybe_tls| {
                if let Some(keepalive) = self.keepalive {
                    if let Err(error) = maybe_tls.set_keepalive(keepalive) {
                        warn!(message = "Failed configuring TCP keepalive.", %error);
                    }
                }

                if let Some(send_buffer_bytes) = self.send_buffer_bytes {
                    if let Err(error) = maybe_tls.set_send_buffer_bytes(send_buffer_bytes) {
                        warn!(message = "Failed configuring send buffer size on TCP socket.", %error);
                    }
                }

                maybe_tls
            })
    }

    async fn connect_backoff(&self) -> MaybeTlsStream<TcpStream> {
        let mut backoff = Self::fresh_backoff();
        loop {
            match self.connect().await {
                Ok(socket) => {
                    emit!(&TcpSocketConnectionEstablished {
                        peer_addr: socket.peer_addr().ok(),
                    });
                    return socket;
                }
                Err(error) => {
                    emit!(&TcpSocketConnectionFailed { error });
                    sleep(backoff.next().unwrap()).await;
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
    acker: Acker,
    encode_event: Arc<dyn Fn(Event) -> Option<Bytes> + Send + Sync>,
}

impl TcpSink {
    fn new(
        connector: TcpConnector,
        acker: Acker,
        encode_event: impl Fn(Event) -> Option<Bytes> + Send + Sync + 'static,
    ) -> Self {
        Self {
            connector,
            acker,
            encode_event: Arc::new(encode_event),
        }
    }

    async fn connect(&self) -> BytesSink<MaybeTlsStream<TcpStream>> {
        let stream = self.connector.connect_backoff().await;
        BytesSink::new(
            stream,
            Self::shutdown_check,
            self.acker.clone(),
            SocketMode::Tcp,
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
        let mut buf = [0u8; 1];
        let mut buf = ReadBuf::new(&mut buf);
        match Pin::new(stream).poll_read(&mut cx, &mut buf) {
            Poll::Ready(Err(error)) => ShutdownCheck::Error(error),
            Poll::Ready(Ok(())) if buf.filled().is_empty() => {
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
impl StreamSink<Event> for TcpSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // We need [Peekable](https://docs.rs/futures/0.3.6/futures/stream/struct.Peekable.html) for initiating
        // connection only when we have something to send.
        let encode_event = Arc::clone(&self.encode_event);
        let mut input = input
            .map(|mut event| {
                let byte_size = event.size_of();
                let finalizers = event.metadata_mut().take_finalizers();
                encode_event(event)
                    .map(|item| EncodedEvent {
                        item,
                        finalizers,
                        byte_size,
                    })
                    .unwrap_or_else(|| EncodedEvent::new(Bytes::new(), 0))
            })
            .peekable();

        while Pin::new(&mut input).peek().await.is_some() {
            let mut sink = self.connect().await;
            let _open_token = OpenGauge::new().open(|count| emit!(&ConnectionOpen { count }));

            let result = match sink
                .send_all_peekable(&mut (&mut input).map(|item| item.item).peekable())
                .await
            {
                Ok(()) => sink.close().await,
                Err(error) => Err(error),
            };

            if let Err(error) = result {
                if error.kind() == ErrorKind::Other && error.to_string() == "ShutdownCheck::Close" {
                    emit!(&TcpSocketConnectionShutdown {});
                } else {
                    emit!(&TcpSocketError { error });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use tokio::net::TcpListener;

    use super::*;
    use crate::test_util::{next_addr, trace_init};

    #[tokio::test]
    async fn healthcheck() {
        trace_init();

        let addr = next_addr();
        let _listener = TcpListener::bind(&addr).await.unwrap();
        let good = TcpConnector::from_host_port(addr.ip().to_string(), addr.port());
        assert!(good.healthcheck().await.is_ok());

        let addr = next_addr();
        let bad = TcpConnector::from_host_port(addr.ip().to_string(), addr.port());
        assert!(bad.healthcheck().await.is_err());
    }
}
