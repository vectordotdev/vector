use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use futures::{future::BoxFuture, ready, stream::BoxStream, FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio::{net::UdpSocket, sync::oneshot, time::sleep};
use vector_buffers::Acker;

use super::SinkBuildError;
use crate::{
    config::SinkContext,
    dns,
    event::Event,
    internal_events::{
        SocketEventsSent, SocketMode, UdpSendIncomplete, UdpSocketConnectionEstablished,
        UdpSocketConnectionFailed, UdpSocketError,
    },
    sinks::{
        util::{retries::ExponentialBackoff, StreamSink},
        Healthcheck, VectorSink,
    },
    udp,
};

#[derive(Debug, Snafu)]
pub enum UdpError {
    #[snafu(display("Failed to create UDP listener socket, error = {:?}.", source))]
    BindError { source: std::io::Error },
    #[snafu(display("Send error: {}", source))]
    SendError { source: std::io::Error },
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: std::io::Error },
    #[snafu(display("No addresses returned."))]
    NoAddresses,
    #[snafu(display("Unable to resolve DNS: {}", source))]
    DnsError { source: crate::dns::DnsError },
    #[snafu(display("Failed to get UdpSocket back: {}", source))]
    ServiceChannelRecvError { source: oneshot::error::RecvError },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UdpSinkConfig {
    address: String,
    send_buffer_bytes: Option<usize>,
}

impl UdpSinkConfig {
    pub const fn from_address(address: String) -> Self {
        Self {
            address,
            send_buffer_bytes: None,
        }
    }

    fn build_connector(&self, _cx: SinkContext) -> crate::Result<UdpConnector> {
        let uri = self.address.parse::<http::Uri>()?;
        let host = uri.host().ok_or(SinkBuildError::MissingHost)?.to_string();
        let port = uri.port_u16().ok_or(SinkBuildError::MissingPort)?;
        Ok(UdpConnector::new(host, port, self.send_buffer_bytes))
    }

    pub fn build_service(&self, cx: SinkContext) -> crate::Result<(UdpService, Healthcheck)> {
        let connector = self.build_connector(cx)?;
        Ok((
            UdpService::new(connector.clone()),
            async move { connector.healthcheck().await }.boxed(),
        ))
    }

    pub fn build(
        &self,
        cx: SinkContext,
        encode_event: impl Fn(Event) -> Option<Bytes> + Send + Sync + 'static,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let connector = self.build_connector(cx.clone())?;
        let sink = UdpSink::new(connector.clone(), cx.acker(), encode_event);
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
            if let Err(error) = udp::set_send_buffer_size(&socket, send_buffer_bytes) {
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
                    emit!(&UdpSocketConnectionEstablished {});
                    return socket;
                }
                Err(error) => {
                    emit!(&UdpSocketConnectionFailed { error });
                    sleep(backoff.next().unwrap()).await;
                }
            }
        }
    }

    async fn healthcheck(&self) -> crate::Result<()> {
        self.connect().await.map(|_| ()).map_err(Into::into)
    }
}

enum UdpServiceState {
    Disconnected,
    Connecting(BoxFuture<'static, UdpSocket>),
    Connected(UdpSocket),
    Sending(oneshot::Receiver<UdpSocket>),
}

pub struct UdpService {
    connector: UdpConnector,
    state: UdpServiceState,
}

impl UdpService {
    const fn new(connector: UdpConnector) -> Self {
        Self {
            connector,
            state: UdpServiceState::Disconnected,
        }
    }
}

impl tower::Service<Bytes> for UdpService {
    type Response = ();
    type Error = UdpError;
    type Future = BoxFuture<'static, Result<(), Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.state = match &mut self.state {
                UdpServiceState::Disconnected => {
                    let connector = self.connector.clone();
                    UdpServiceState::Connecting(Box::pin(async move {
                        connector.connect_backoff().await
                    }))
                }
                UdpServiceState::Connecting(fut) => {
                    let socket = ready!(fut.poll_unpin(cx));
                    UdpServiceState::Connected(socket)
                }
                UdpServiceState::Connected(_) => break,
                UdpServiceState::Sending(fut) => {
                    let socket = match ready!(fut.poll_unpin(cx)).context(ServiceChannelRecvSnafu) {
                        Ok(socket) => socket,
                        Err(error) => return Poll::Ready(Err(error)),
                    };
                    UdpServiceState::Connected(socket)
                }
            };
        }
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Bytes) -> Self::Future {
        let (sender, receiver) = oneshot::channel();

        let mut socket =
            match std::mem::replace(&mut self.state, UdpServiceState::Sending(receiver)) {
                UdpServiceState::Connected(socket) => socket,
                _ => panic!("UdpService::poll_ready should be called first"),
            };

        Box::pin(async move {
            // TODO: Add reconnect support as TCP/Unix?
            let result = udp_send(&mut socket, &msg).await.context(SendSnafu);
            let _ = sender.send(socket);
            result
        })
    }
}

struct UdpSink {
    connector: UdpConnector,
    acker: Acker,
    encode_event: Box<dyn Fn(Event) -> Option<Bytes> + Send + Sync>,
}

impl UdpSink {
    fn new(
        connector: UdpConnector,
        acker: Acker,
        encode_event: impl Fn(Event) -> Option<Bytes> + Send + Sync + 'static,
    ) -> Self {
        Self {
            connector,
            acker,
            encode_event: Box::new(encode_event),
        }
    }
}

#[async_trait]
impl StreamSink<Event> for UdpSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mut input = input.peekable();

        while Pin::new(&mut input).peek().await.is_some() {
            let mut socket = self.connector.connect_backoff().await;
            while let Some(event) = input.next().await {
                self.acker.ack(1);

                let input = match (self.encode_event)(event) {
                    Some(input) => input,
                    None => continue,
                };

                match udp_send(&mut socket, &input).await {
                    Ok(()) => emit!(&SocketEventsSent {
                        mode: SocketMode::Udp,
                        count: 1,
                        byte_size: input.len(),
                    }),
                    Err(error) => {
                        emit!(&UdpSocketError { error });
                        break;
                    }
                };
            }
        }

        Ok(())
    }
}

async fn udp_send(socket: &mut UdpSocket, buf: &[u8]) -> tokio::io::Result<()> {
    let sent = socket.send(buf).await?;
    if sent != buf.len() {
        emit!(&UdpSendIncomplete {
            data_size: buf.len(),
            sent,
        });
    }
    Ok(())
}

fn find_bind_address(remote_addr: &SocketAddr) -> SocketAddr {
    match remote_addr {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}
