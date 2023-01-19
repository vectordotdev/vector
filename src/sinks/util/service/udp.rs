use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    task::{ready, Context, Poll},
    time::Duration,
};

use futures::{future::BoxFuture, FutureExt};
use snafu::{ResultExt, Snafu};
use tokio::{net::UdpSocket, sync::oneshot, time::sleep};
use tower::Service;

use crate::{
    dns,
    internal_events::{
        UdpSendIncompleteError, UdpSocketConnectionEstablished, UdpSocketOutgoingConnectionError,
    },
    sinks::{util::retries::ExponentialBackoff, Healthcheck},
    udp,
};

#[derive(Debug, Snafu)]
pub enum UdpError {
    #[snafu(display("Address was invalid: {}", reason))]
    InvalidAddress { reason: &'static str },

    #[snafu(display("Failed to bind UDP socket: {}.", source))]
    FailedToBind { source: std::io::Error },

    #[snafu(display("Failed to send UDP datagram: {}", source))]
    FailedToSend { source: std::io::Error },

    #[snafu(display("Failed to connect to UDP endpoint: {}", source))]
    FailedToConnect { source: std::io::Error },

    #[snafu(display("No addresses returned."))]
    NoAddresses,

    #[snafu(display("Failed to resolve address: {}", source))]
    FailedToResolve { source: crate::dns::DnsError },

    #[snafu(display("Failed to get UDP socket back after send as channel closed unexpectedly."))]
    ServiceSocketChannelClosed,
}

#[derive(Clone)]
pub struct UdpConnector {
    host: String,
    port: u16,
    send_buffer_size: Option<usize>,
}

impl UdpConnector {
    /// Creates a new `UdpConnector` configured to send to the given address.
    ///
    /// The `address` must be a valid URI containing both the host and port to send to.
    pub fn new(address: String) -> crate::Result<Self> {
        let uri = address.parse::<http::Uri>()?;
        let host = uri
            .host()
            .ok_or(UdpError::InvalidAddress {
                reason: "missing host",
            })?
            .to_string();
        let port = uri.port_u16().ok_or(UdpError::InvalidAddress {
            reason: "missing port",
        })?;

        Ok(Self {
            host,
            port,
            send_buffer_size: None,
        })
    }

    /// Sets the size of the socket send buffer, in bytes.
    ///
    /// This configures the `SO_SNDBUF` option on the socket.
    pub fn set_send_buffer_size(&mut self, size: usize) {
        self.send_buffer_size = Some(size);
    }

    async fn connect(&self) -> Result<UdpSocket, UdpError> {
        let ip = dns::Resolver
            .lookup_ip(self.host.clone())
            .await
            .context(FailedToResolveSnafu)?
            .next()
            .ok_or(UdpError::NoAddresses)?;

        let addr = SocketAddr::new(ip, self.port);
        let bind_address = find_bind_address(&addr);

        let socket = UdpSocket::bind(bind_address)
            .await
            .context(FailedToBindSnafu)?;

        if let Some(send_buffer_size) = self.send_buffer_size {
            if let Err(error) = udp::set_send_buffer_size(&socket, send_buffer_size) {
                warn!(%error, "Failed configuring send buffer size on UDP socket.");
            }
        }

        socket.connect(addr).await.context(FailedToConnectSnafu)?;

        Ok(socket)
    }

    async fn connect_backoff(&self) -> UdpSocket {
        // TODO: Make this configurable.
        let mut backoff = ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60));

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

enum UdpServiceState {
    /// The service is currently disconnected.
    Disconnected,

    /// The service is currently attempting to connect to the endpoint.
    Connecting(BoxFuture<'static, UdpSocket>),

    /// The service is connected and idle.
    Connected(UdpSocket),

    /// The service has an in-flight send to the socket.
    ///
    /// If the socket experiences an unrecoverable error during the send, `None` will be returned
    /// over the channel to signal the need to establish a new connection rather than reusing the
    /// existing connection.
    Sending(oneshot::Receiver<Option<UdpSocket>>),
}

pub struct UdpService {
    connector: UdpConnector,
    state: UdpServiceState,
}

impl UdpService {
    fn new(connector: UdpConnector) -> Self {
        Self {
            connector,
            state: UdpServiceState::Disconnected,
        }
    }
}

impl Service<Vec<u8>> for UdpService {
    type Response = usize;
    type Error = UdpError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

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
                    match ready!(fut.poll_unpin(cx)) {
                        // When a send concludes, and there's an error, the request future sends
                        // back `None`. Otherwise, it'll send back `Some(...)` with the socket.
                        Ok(maybe_socket) => match maybe_socket {
                            Some(socket) => UdpServiceState::Connected(socket),
                            None => UdpServiceState::Disconnected,
                        },
                        Err(_) => return Poll::Ready(Err(UdpError::ServiceSocketChannelClosed)),
                    }
                }
            };
        }
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, buf: Vec<u8>) -> Self::Future {
        let (tx, rx) = oneshot::channel();

        let mut socket = match std::mem::replace(&mut self.state, UdpServiceState::Sending(rx)) {
            UdpServiceState::Connected(socket) => socket,
            _ => panic!("poll_ready must be called first"),
        };

        Box::pin(async move {
            match socket.send(&buf).await.context(FailedToSendSnafu) {
                Ok(sent) => {
                    // Emit an error if we weren't able to send the entire buffer.
                    if sent != buf.len() {
                        emit!(UdpSendIncompleteError {
                            data_size: buf.len(),
                            sent,
                        });
                    }

                    // Send the socket back to the service, since theoretically it's still valid to
                    // reuse given that we may have simply overrun the OS socket buffers, etc.
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
