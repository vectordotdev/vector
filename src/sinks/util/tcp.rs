use std::{
    io::ErrorKind,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures::{stream::BoxStream, task::noop_waker_ref, SinkExt, StreamExt};
use futures_util::{future::ready, stream};
use snafu::{ResultExt, Snafu};
use tokio::{
    io::{AsyncRead, ReadBuf},
    net::TcpStream,
    time::sleep,
};
use tokio_util::codec::Encoder;
use vector_lib::configurable::configurable_component;
use vector_lib::json_size::JsonSize;
use vector_lib::{ByteSizeOf, EstimatedJsonEncodedSizeOf};

use crate::{
    codecs::Transformer,
    dns,
    event::Event,
    internal_events::{
        ConnectionOpen, OpenGauge, SocketMode, SocketSendError, TcpSocketConnectionEstablished,
        TcpSocketConnectionShutdown, TcpSocketOutgoingConnectionError,
    },
    sinks::{
        util::{
            retries::ExponentialBackoff,
            socket_bytes_sink::{BytesSink, ShutdownCheck},
            EncodedEvent, SinkBuildError, StreamSink,
        },
        Healthcheck, VectorSink,
    },
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, MaybeTlsStream, TlsEnableableConfig, TlsError},
};

#[derive(Debug, Snafu)]
enum TcpError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: TlsError },
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: dns::DnsError },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
}

/// A TCP sink.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct TcpSinkConfig {
    /// The address to connect to.
    ///
    /// Both IP address and hostname are accepted formats.
    ///
    /// The address _must_ include a port.
    #[configurable(metadata(docs::examples = "92.12.333.224:5000"))]
    #[configurable(metadata(docs::examples = "https://somehost:5000"))]
    address: String,

    #[configurable(derived)]
    keepalive: Option<TcpKeepaliveConfig>,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    /// The size of the socket's send buffer.
    ///
    /// If set, the value of the setting is passed via the `SO_SNDBUF` option.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 65536))]
    send_buffer_bytes: Option<usize>,
}

impl TcpSinkConfig {
    pub const fn new(
        address: String,
        keepalive: Option<TcpKeepaliveConfig>,
        tls: Option<TlsEnableableConfig>,
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
        transformer: Transformer,
        encoder: impl Encoder<Event, Error = vector_lib::codecs::encoding::Error>
            + Clone
            + Send
            + Sync
            + 'static,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let uri = self.address.parse::<http::Uri>()?;
        let host = uri.host().ok_or(SinkBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(SinkBuildError::MissingPort)?;
        let tls = MaybeTlsSettings::from_config(&self.tls, false)?;
        let connector = TcpConnector::new(host, port, self.keepalive, tls, self.send_buffer_bytes);
        let sink = TcpSink::new(connector.clone(), transformer, encoder);

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
                    emit!(TcpSocketConnectionEstablished {
                        peer_addr: socket.peer_addr().ok(),
                    });
                    return socket;
                }
                Err(error) => {
                    emit!(TcpSocketOutgoingConnectionError { error });
                    sleep(backoff.next().unwrap()).await;
                }
            }
        }
    }

    async fn healthcheck(&self) -> crate::Result<()> {
        self.connect().await.map(|_| ()).map_err(Into::into)
    }
}

struct TcpSink<E>
where
    E: Encoder<Event, Error = vector_lib::codecs::encoding::Error> + Clone + Send + Sync,
{
    connector: TcpConnector,
    transformer: Transformer,
    encoder: E,
}

impl<E> TcpSink<E>
where
    E: Encoder<Event, Error = vector_lib::codecs::encoding::Error> + Clone + Send + Sync + 'static,
{
    const fn new(connector: TcpConnector, transformer: Transformer, encoder: E) -> Self {
        Self {
            connector,
            transformer,
            encoder,
        }
    }

    async fn connect(&self) -> BytesSink<MaybeTlsStream<TcpStream>> {
        let stream = self.connector.connect_backoff().await;
        BytesSink::new(stream, Self::shutdown_check, SocketMode::Tcp)
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
impl<E> StreamSink<Event> for TcpSink<E>
where
    E: Encoder<Event, Error = vector_lib::codecs::encoding::Error>
        + Clone
        + Send
        + Sync
        + Sync
        + 'static,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // We need [Peekable](https://docs.rs/futures/0.3.6/futures/stream/struct.Peekable.html) for initiating
        // connection only when we have something to send.
        let mut encoder = self.encoder.clone();
        let mut input = input.map(|mut event| {
            let byte_size = event.size_of();
            let json_byte_size = event.estimated_json_encoded_size_of();
            let finalizers = event.metadata_mut().take_finalizers();
            self.transformer.transform(&mut event);
            let mut bytes = BytesMut::new();

            // Errors are handled by `Encoder`.
            if encoder.encode(event, &mut bytes).is_ok() {
                let item = bytes.freeze();
                EncodedEvent {
                    item,
                    finalizers,
                    byte_size,
                    json_byte_size,
                }
            } else {
                EncodedEvent::new(Bytes::new(), 0, JsonSize::zero())
            }
        });

        while let Some(item) = input.next().await {
            let mut sink = self.connect().await;
            let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));

            let mut mapped_input = stream::once(ready(item)).chain(&mut input).map(Ok);

            let result = match sink.send_all(&mut mapped_input).await {
                Ok(()) => sink.close().await,
                Err(error) => Err(error),
            };

            // TODO we can consider retrying once in the Error case. This sink is a "best effort"
            // delivery due to the nature of the underlying protocol.
            // For now, if an error occurs we cannot assume that the events succeeded in delivery
            // so we will emit `Error` / `EventsDropped` internal events regardless of if the server
            // responded with Ok(0).
            if let Err(error) = result {
                if error.kind() == ErrorKind::Other && error.to_string() == "ShutdownCheck::Close" {
                    emit!(TcpSocketConnectionShutdown {});
                }
                emit!(SocketSendError {
                    mode: SocketMode::Tcp,
                    error
                });
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
