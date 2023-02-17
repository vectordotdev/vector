use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    task::{ready, Context, Poll},
    time::Duration,
};

use futures::future::BoxFuture;
use futures_util::FutureExt;
use snafu::ResultExt;
use tokio::{net::UdpSocket, sync::oneshot, time::sleep};
use tower::Service;

use vector_config::configurable_component;

use crate::{
    dns,
    internal_events::{UdpSendIncompleteError, UdpSocketOutgoingConnectionError},
    net,
    sinks::{util::retries::ExponentialBackoff, Healthcheck},
};

use super::{net_error::*, HostAndPort, NetError, ServiceState};

/// `UdpConnector` configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UdpConnectorConfig {
    /// The address to connect to.
    ///
    /// Both IP addresses and hostnames/fully-qualified domain names are accepted formats.
    ///
    /// The address _must_ include a port.
    address: HostAndPort,

    /// The size of the socket's send buffer, in bytes.
    ///
    /// If set, the value of the setting is passed via the `SO_SNDBUF` option.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 65536))]
    send_buffer_size: Option<usize>,
}

impl UdpConnectorConfig {
    pub const fn from_address(host: String, port: u16) -> Self {
        Self {
            address: HostAndPort { host, port },
            send_buffer_size: None,
        }
    }

    pub fn as_connector(&self) -> UdpConnector {
        UdpConnector {
            address: self.address.clone(),
            send_buffer_size: self.send_buffer_size,
        }
    }
}

#[derive(Clone)]
pub struct UdpConnector {
    address: HostAndPort,
    send_buffer_size: Option<usize>,
}

impl UdpConnector {
    async fn connect(&self) -> Result<UdpSocket, NetError> {
        let ip = dns::Resolver
            .lookup_ip(self.address.host.clone())
            .await
            .context(FailedToResolve)?
            .next()
            .ok_or(NetError::NoAddresses)?;

        let addr = SocketAddr::new(ip, self.address.port);
        let bind_address = find_bind_address(&addr);

        let socket = UdpSocket::bind(bind_address).await.context(FailedToBind)?;

        if let Some(send_buffer_size) = self.send_buffer_size {
            if let Err(error) = net::set_send_buffer_size(&socket, send_buffer_size) {
                warn!(%error, "Failed configuring send buffer size on UDP socket.");
            }
        }

        socket.connect(addr).await.context(FailedToConnect)?;

        Ok(socket)
    }

    async fn connect_backoff(&self) -> UdpSocket {
        // TODO: Make this configurable.
        let mut backoff = ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60));

        loop {
            match self.connect().await {
                Ok(socket) => return socket,
                Err(error) => {
                    emit!(UdpSocketOutgoingConnectionError { error });
                    sleep(backoff.next().unwrap()).await;
                }
            }
        }
    }

    /// Gets a `Healthcheck` based on the configured destination of this connector.
    pub fn healthcheck(&self) -> Healthcheck {
        let connector = self.clone();
        Box::pin(async move { connector.connect().await.map(|_| ()).map_err(Into::into) })
    }

    /// Gets a `Service` suitable for sending data to the configured destination of this connector.
    pub fn service(&self) -> UdpService {
        UdpService::new(self.clone())
    }
}

pub struct UdpService {
    connector: UdpConnector,
    state: ServiceState<UdpSocket>,
}

impl UdpService {
    const fn new(connector: UdpConnector) -> Self {
        Self {
            connector,
            state: ServiceState::Disconnected,
        }
    }
}

impl Service<Vec<u8>> for UdpService {
    type Response = usize;
    type Error = NetError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.state = match &mut self.state {
                ServiceState::Disconnected => {
                    let connector = self.connector.clone();
                    ServiceState::Connecting(Box::pin(
                        async move { connector.connect_backoff().await },
                    ))
                }
                ServiceState::Connecting(fut) => {
                    let socket = ready!(fut.poll_unpin(cx));
                    ServiceState::Connected(socket)
                }
                ServiceState::Connected(_) => break,
                ServiceState::Sending(fut) => {
                    match ready!(fut.poll_unpin(cx)) {
                        // When a send concludes, and there's an error, the request future sends
                        // back `None`. Otherwise, it'll send back `Some(...)` with the socket.
                        Ok(maybe_socket) => match maybe_socket {
                            Some(socket) => ServiceState::Connected(socket),
                            None => ServiceState::Disconnected,
                        },
                        Err(_) => return Poll::Ready(Err(NetError::ServiceSocketChannelClosed)),
                    }
                }
            };
        }
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, buf: Vec<u8>) -> Self::Future {
        let (tx, rx) = oneshot::channel();

        let socket = match std::mem::replace(&mut self.state, ServiceState::Sending(rx)) {
            ServiceState::Connected(socket) => socket,
            _ => panic!("poll_ready must be called first"),
        };

        Box::pin(async move {
            match socket.send(&buf).await.context(FailedToSend) {
                Ok(sent) => {
                    // Emit an error if we weren't able to send the entire buffer.
                    if sent != buf.len() {
                        emit!(UdpSendIncompleteError {
                            data_size: buf.len(),
                            sent,
                        });
                    }

                    // Send the socket back to the service no matter what, since theoretically it's
                    // still valid to reuse even if we didn't send all of the buffer, as we may have
                    // simply overrun the OS socket buffers, etc.
                    let _ = tx.send(Some(socket));

                    Ok(sent)
                }
                Err(e) => {
                    // We need to signal back to the service that it needs to create a fresh socket
                    // since this one could be tainted.
                    let _ = tx.send(None);

                    Err(e)
                }
            }
        })
    }
}

fn find_bind_address(remote_addr: &SocketAddr) -> SocketAddr {
    match remote_addr {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}
