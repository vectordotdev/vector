use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    pin::Pin,
    time::Duration,
};

use async_trait::async_trait;
use bytes::BytesMut;
use futures::{stream::BoxStream, FutureExt, StreamExt};
use snafu::{ResultExt, Snafu};
use tokio::{net::UdpSocket, time::sleep};
use tokio_util::codec::Encoder;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{ByteSize, BytesSent, InternalEventHandle, Protocol, Registered};
use vector_lib::EstimatedJsonEncodedSizeOf;

use super::SinkBuildError;
use crate::{
    codecs::Transformer,
    dns,
    event::{Event, EventStatus, Finalizable},
    internal_events::{
        SocketEventsSent, SocketMode, SocketSendError, UdpSendIncompleteError,
        UdpSocketConnectionEstablished, UdpSocketOutgoingConnectionError,
    },
    net,
    sinks::{
        util::{retries::ExponentialBackoff, StreamSink},
        Healthcheck, VectorSink,
    },
};

#[derive(Debug, Snafu)]
pub enum UdpError {
    #[snafu(display("Failed to create UDP listener socket, error = {:?}.", source))]
    BindError { source: std::io::Error },
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: std::io::Error },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: crate::dns::DnsError },
}

/// A UDP sink.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UdpSinkConfig {
    /// The address to connect to.
    ///
    /// Both IP address and hostname are accepted formats.
    ///
    /// The address _must_ include a port.
    #[configurable(metadata(docs::examples = "92.12.333.224:5000"))]
    #[configurable(metadata(docs::examples = "https://somehost:5000"))]
    address: String,

    /// The size of the socket's send buffer.
    ///
    /// If set, the value of the setting is passed via the `SO_SNDBUF` option.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 65536))]
    send_buffer_bytes: Option<usize>,
}

impl UdpSinkConfig {
    pub const fn from_address(address: String) -> Self {
        Self {
            address,
            send_buffer_bytes: None,
        }
    }

    fn build_connector(&self) -> crate::Result<UdpConnector> {
        let uri = self.address.parse::<http::Uri>()?;
        let host = uri.host().ok_or(SinkBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(SinkBuildError::MissingPort)?;
        Ok(UdpConnector::new(host, port, self.send_buffer_bytes))
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
        let connector = self.build_connector()?;
        let sink = UdpSink::new(connector.clone(), transformer, encoder);
        Ok((
            VectorSink::from_event_streamsink(sink),
            async move { connector.healthcheck().await }.boxed(),
        ))
    }
}

#[derive(Clone)]
struct UdpConnector {
    host: String,
    port: u16,
    send_buffer_bytes: Option<usize>,
}

impl UdpConnector {
    const fn new(host: String, port: u16, send_buffer_bytes: Option<usize>) -> Self {
        Self {
            host,
            port,
            send_buffer_bytes,
        }
    }

    const fn fresh_backoff() -> ExponentialBackoff {
        // TODO: make configurable
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }

    async fn connect(&self) -> Result<UdpSocket, UdpError> {
        let ip = dns::Resolver
            .lookup_ip(self.host.clone())
            .await
            .context(DnsSnafu)?
            .next()
            .ok_or(UdpError::NoAddresses)?;

        let addr = SocketAddr::new(ip, self.port);
        let bind_address = find_bind_address(&addr);

        let socket = UdpSocket::bind(bind_address).await.context(BindSnafu)?;

        if let Some(send_buffer_bytes) = self.send_buffer_bytes {
            if let Err(error) = net::set_send_buffer_size(&socket, send_buffer_bytes) {
                warn!(message = "Failed configuring send buffer size on UDP socket.", %error);
            }
        }

        socket.connect(addr).await.context(ConnectSnafu)?;

        Ok(socket)
    }

    async fn connect_backoff(&self) -> UdpSocket {
        let mut backoff = Self::fresh_backoff();
        loop {
            match self.connect().await {
                Ok(socket) => {
                    emit!(UdpSocketConnectionEstablished {});
                    return socket;
                }
                Err(error) => {
                    emit!(UdpSocketOutgoingConnectionError { error });
                    sleep(backoff.next().unwrap()).await;
                }
            }
        }
    }

    async fn healthcheck(&self) -> crate::Result<()> {
        self.connect().await.map(|_| ()).map_err(Into::into)
    }
}

struct UdpSink<E>
where
    E: Encoder<Event, Error = vector_lib::codecs::encoding::Error> + Clone + Send + Sync,
{
    connector: UdpConnector,
    transformer: Transformer,
    encoder: E,
    bytes_sent: Registered<BytesSent>,
}

impl<E> UdpSink<E>
where
    E: Encoder<Event, Error = vector_lib::codecs::encoding::Error> + Clone + Send + Sync,
{
    fn new(connector: UdpConnector, transformer: Transformer, encoder: E) -> Self {
        Self {
            connector,
            transformer,
            encoder,
            bytes_sent: register!(BytesSent::from(Protocol::UDP)),
        }
    }
}

#[async_trait]
impl<E> StreamSink<Event> for UdpSink<E>
where
    E: Encoder<Event, Error = vector_lib::codecs::encoding::Error> + Clone + Send + Sync,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mut input = input.peekable();

        let mut encoder = self.encoder.clone();
        while Pin::new(&mut input).peek().await.is_some() {
            let mut socket = self.connector.connect_backoff().await;
            while let Some(mut event) = input.next().await {
                let byte_size = event.estimated_json_encoded_size_of();

                self.transformer.transform(&mut event);

                let finalizers = event.take_finalizers();
                let mut bytes = BytesMut::new();

                // Errors are handled by `Encoder`.
                if encoder.encode(event, &mut bytes).is_err() {
                    continue;
                }

                match udp_send(&mut socket, &bytes).await {
                    Ok(()) => {
                        emit!(SocketEventsSent {
                            mode: SocketMode::Udp,
                            count: 1,
                            byte_size,
                        });

                        self.bytes_sent.emit(ByteSize(bytes.len()));
                        finalizers.update_status(EventStatus::Delivered);
                    }
                    Err(error) => {
                        emit!(SocketSendError {
                            mode: SocketMode::Udp,
                            error
                        });
                        finalizers.update_status(EventStatus::Errored);
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}

async fn udp_send(socket: &mut UdpSocket, buf: &[u8]) -> tokio::io::Result<()> {
    let sent = socket.send(buf).await?;
    if sent != buf.len() {
        emit!(UdpSendIncompleteError {
            data_size: buf.len(),
            sent,
        });
    }
    Ok(())
}

pub(super) const fn find_bind_address(remote_addr: &SocketAddr) -> SocketAddr {
    match remote_addr {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}
