mod tcp;
mod udp;
mod unix;

use std::{io, task::{Context, Poll, ready}};

pub use self::tcp::{TcpConnector, TcpConnectorConfig};
pub use self::udp::{UdpConnector, UdpConnectorConfig};
use self::{unix::UnixEither, net_error::FailedToSend};
pub use self::unix::{UnixConnector, UnixConnectorConfig, UnixMode};

use futures_util::{future::BoxFuture, FutureExt};
use snafu::{Snafu, ResultExt};
use tokio::{sync::oneshot, net::{TcpStream, UdpSocket}, io::AsyncWriteExt};
use tower::Service;
use vector_config::configurable_component;

/// Hostname and port tuple.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(try_from = "String", into = "String")]
#[configurable(metadata(docs::examples = "92.12.333.224:5000"))]
#[configurable(metadata(docs::examples = "somehost:5000"))]
struct HostAndPort {
    /// Hostname.
    host: String,

    /// Port.
    port: u16,
}

impl TryFrom<String> for HostAndPort {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let uri = value.parse::<http::Uri>().map_err(|e| e.to_string())?;
        let host = uri
            .host()
            .ok_or_else(|| "missing host".to_string())?
            .to_string();
        let port = uri.port_u16().ok_or_else(|| "missing port".to_string())?;

        Ok(Self { host, port })
    }
}

impl From<HostAndPort> for String {
    fn from(value: HostAndPort) -> Self {
        format!("{}:{}", value.host, value.port)
    }
}

#[derive(Debug, Snafu)]
#[snafu(module, context(suffix(false)), visibility(pub))]
pub enum NetError {
    #[snafu(display("Address is invalid: {}", reason))]
    InvalidAddress { reason: String },

    #[snafu(display("Failed to resolve address: {}", source))]
    FailedToResolve { source: crate::dns::DnsError },

    #[snafu(display("No addresses returned."))]
    NoAddresses,

    #[snafu(display("Failed to configure socket: {}.", source))]
    FailedToConfigure { source: std::io::Error },

    #[snafu(display("Failed to bind socket: {}.", source))]
    FailedToBind { source: std::io::Error },

    #[snafu(display("Failed to send message: {}", source))]
    FailedToSend { source: std::io::Error },

    #[snafu(display("Failed to connect to endpoint: {}", source))]
    FailedToConnect { source: std::io::Error },

    #[snafu(display("Failed to get socket back after send as channel closed unexpectedly."))]
    ServiceSocketChannelClosed,
}

pub enum ServiceState<C> {
    /// The service is currently disconnected.
    Disconnected,

    /// The service is currently attempting to connect to the endpoint.
    Connecting(BoxFuture<'static, C>),

    /// The service is connected and idle.
    Connected(C),

    /// The service has an in-flight send to the socket.
    ///
    /// If the socket experiences an unrecoverable error during the send, `None` will be returned
    /// over the channel to signal the need to establish a new connection rather than reusing the
    /// existing connection.
    Sending(oneshot::Receiver<Option<C>>),
}

pub enum ServiceState2 {
    /// The service is currently disconnected.
    Disconnected,

    /// The service is currently attempting to connect to the endpoint.
    Connecting(BoxFuture<'static, NetworkConnection>),

    /// The service is connected and idle.
    Connected(NetworkConnection),

    /// The service has an in-flight send to the socket.
    ///
    /// If the socket experiences an unrecoverable error during the send, `None` will be returned
    /// over the channel to signal the need to establish a new connection rather than reusing the
    /// existing connection.
    Sending(oneshot::Receiver<Option<NetworkConnection>>),
}

pub enum NetworkConnection {
    Tcp(TcpStream),
    Udp(UdpSocket),
    Unix(UnixEither),
}

impl NetworkConnection {
    pub async fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.write_all(buf).await
                .map(|()| buf.len()),
            Self::Udp(socket) => socket.send(buf).await,
            Self::Unix(socket) => socket.send(buf).await,
        }
    }
}

pub enum NetworkConnector {
    Tcp(TcpConnector),
    Udp(UdpConnector),
    Unix(UnixConnector),
}

impl NetworkConnector {
    async fn connect_backoff(&self) -> NetworkConnection {
        // TODO: Make this configurable.
        let mut backoff = ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60));

        loop {
            match self.connect().await {
                Ok((addr, stream)) => {
                    emit!(TcpSocketConnectionEstablished {
                        peer_addr: Some(addr)
                    });
                    return stream;
                }
                Err(error) => {
                    emit!(TcpSocketOutgoingConnectionError { error });
                    sleep(backoff.next().unwrap()).await;
                }
            }
        }
    }
}

pub struct NetworkService {
    connector: NetworkConnector,
    state: ServiceState2,
}

impl NetworkService {
    const fn new(connector: NetworkConnector) -> Self {
        Self {
            connector,
            state: ServiceState2::Disconnected,
        }
    }
}

impl Service<Vec<u8>> for NetworkService {
    type Response = usize;
    type Error = NetError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.state = match &mut self.state {
                ServiceState2::Disconnected => {
                    let connector = self.connector.clone();
                    ServiceState2::Connecting(Box::pin(
                        async move { connector.connect_backoff().await },
                    ))
                }
                ServiceState2::Connecting(fut) => {
                    let socket = ready!(fut.poll_unpin(cx));
                    ServiceState2::Connected(socket)
                }
                ServiceState2::Connected(_) => break,
                ServiceState2::Sending(fut) => {
                    match ready!(fut.poll_unpin(cx)) {
                        // When a send concludes, and there's an error, the request future sends
                        // back `None`. Otherwise, it'll send back `Some(...)` with the socket.
                        Ok(maybe_socket) => match maybe_socket {
                            Some(socket) => ServiceState2::Connected(socket),
                            None => ServiceState2::Disconnected,
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

        let mut socket = match std::mem::replace(&mut self.state, ServiceState2::Sending(rx)) {
            ServiceState2::Connected(socket) => socket,
            _ => panic!("poll_ready must be called first"),
        };

        Box::pin(async move {
            match socket.send(&buf).await.context(FailedToSend) {
                Ok(sent) => {
                    // Emit an error if we weren't able to send the entire buffer.
                    if sent != buf.len() {
                        emit!(UnixSendIncompleteError {
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
