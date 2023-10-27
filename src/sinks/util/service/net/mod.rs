mod tcp;
mod udp;

#[cfg(unix)]
mod unix;

use std::{
    io,
    net::SocketAddr,
    task::{ready, Context, Poll},
    time::Duration,
};

#[cfg(unix)]
use std::path::PathBuf;

use crate::{
    internal_events::{
        SocketOutgoingConnectionError, TcpSocketConnectionEstablished, UdpSendIncompleteError,
    },
    sinks::{util::retries::ExponentialBackoff, Healthcheck},
};

#[cfg(unix)]
use crate::internal_events::{UnixSendIncompleteError, UnixSocketConnectionEstablished};

pub use self::tcp::TcpConnectorConfig;
pub use self::udp::UdpConnectorConfig;

#[cfg(unix)]
pub use self::unix::{UnixConnectorConfig, UnixMode};

use self::tcp::TcpConnector;
use self::udp::UdpConnector;
#[cfg(unix)]
use self::unix::{UnixConnector, UnixEither};

use futures_util::{future::BoxFuture, FutureExt};
use snafu::{ResultExt, Snafu};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpStream, UdpSocket},
    sync::oneshot,
    time::sleep,
};
use tower::Service;
use vector_lib::configurable::configurable_component;
use vector_lib::tls::{MaybeTlsStream, TlsError};

/// Hostname and port tuple.
///
/// Both IP addresses and hostnames/fully qualified domain names (FQDNs) are accepted formats.
///
/// The address _must_ include a port.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(try_from = "String", into = "String")]
#[configurable(title = "The address to connect to.")]
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

    #[snafu(display("Failed to configure TLS: {}.", source))]
    FailedToConfigureTLS { source: TlsError },

    #[snafu(display("Failed to bind socket: {}.", source))]
    FailedToBind { source: std::io::Error },

    #[snafu(display("Failed to send message: {}", source))]
    FailedToSend { source: std::io::Error },

    #[snafu(display("Failed to connect to endpoint: {}", source))]
    FailedToConnect { source: std::io::Error },

    #[snafu(display("Failed to connect to TLS endpoint: {}", source))]
    FailedToConnectTLS { source: TlsError },

    #[snafu(display("Failed to get socket back after send as channel closed unexpectedly."))]
    ServiceSocketChannelClosed,
}

enum NetworkServiceState {
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

enum NetworkConnection {
    Tcp(MaybeTlsStream<TcpStream>),
    Udp(UdpSocket),
    #[cfg(unix)]
    Unix(UnixEither),
}

impl NetworkConnection {
    fn on_partial_send(&self, data_size: usize, sent: usize) {
        match self {
            // Can't "successfully" partially send with TCP: it either all eventually sends or the
            // socket has an I/O error that kills the connection entirely.
            Self::Tcp(_) => {}
            Self::Udp(_) => {
                emit!(UdpSendIncompleteError { data_size, sent });
            }
            #[cfg(unix)]
            Self::Unix(_) => {
                emit!(UnixSendIncompleteError { data_size, sent });
            }
        }
    }

    async fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.write_all(buf).await.map(|()| buf.len()),
            Self::Udp(socket) => socket.send(buf).await,
            #[cfg(unix)]
            Self::Unix(socket) => socket.send(buf).await,
        }
    }
}

enum ConnectionMetadata {
    Tcp {
        peer_addr: SocketAddr,
    },
    #[cfg(unix)]
    Unix {
        path: PathBuf,
    },
}

#[derive(Clone)]
enum ConnectorType {
    Tcp(TcpConnector),
    Udp(UdpConnector),
    #[cfg(unix)]
    Unix(UnixConnector),
}

/// A connector for generically connecting to a remote network endpoint.
///
/// The connection can be based on TCP, UDP, or Unix Domain Sockets.
#[derive(Clone)]
pub struct NetworkConnector {
    inner: ConnectorType,
}

impl NetworkConnector {
    fn on_connected(&self, metadata: ConnectionMetadata) {
        match metadata {
            ConnectionMetadata::Tcp { peer_addr } => {
                emit!(TcpSocketConnectionEstablished {
                    peer_addr: Some(peer_addr)
                });
            }
            #[cfg(unix)]
            ConnectionMetadata::Unix { path } => {
                emit!(UnixSocketConnectionEstablished { path: &path });
            }
        }
    }

    fn on_connection_error<E: std::error::Error>(&self, error: E) {
        emit!(SocketOutgoingConnectionError { error });
    }

    async fn connect(&self) -> Result<(NetworkConnection, Option<ConnectionMetadata>), NetError> {
        match &self.inner {
            ConnectorType::Tcp(connector) => {
                let (peer_addr, stream) = connector.connect().await?;

                Ok((
                    NetworkConnection::Tcp(stream),
                    Some(ConnectionMetadata::Tcp { peer_addr }),
                ))
            }
            ConnectorType::Udp(connector) => {
                let socket = connector.connect().await?;

                Ok((NetworkConnection::Udp(socket), None))
            }
            #[cfg(unix)]
            ConnectorType::Unix(connector) => {
                let (path, socket) = connector.connect().await?;

                Ok((
                    NetworkConnection::Unix(socket),
                    Some(ConnectionMetadata::Unix { path }),
                ))
            }
        }
    }

    async fn connect_backoff(&self) -> NetworkConnection {
        // TODO: Make this configurable.
        let mut backoff = ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60));

        loop {
            match self.connect().await {
                Ok((connection, maybe_metadata)) => {
                    if let Some(metadata) = maybe_metadata {
                        self.on_connected(metadata);
                    }

                    return connection;
                }
                Err(error) => {
                    self.on_connection_error(error);
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
    pub fn service(&self) -> NetworkService {
        NetworkService::new(self.clone())
    }
}

/// A `Service` implementation for generically sending bytes to a remote peer over a network connection.
///
/// The connection can be based on TCP, UDP, or Unix Domain Sockets.
pub struct NetworkService {
    connector: NetworkConnector,
    state: NetworkServiceState,
}

impl NetworkService {
    const fn new(connector: NetworkConnector) -> Self {
        Self {
            connector,
            state: NetworkServiceState::Disconnected,
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
                NetworkServiceState::Disconnected => {
                    let connector = self.connector.clone();
                    NetworkServiceState::Connecting(Box::pin(async move {
                        connector.connect_backoff().await
                    }))
                }
                NetworkServiceState::Connecting(fut) => {
                    let socket = ready!(fut.poll_unpin(cx));
                    NetworkServiceState::Connected(socket)
                }
                NetworkServiceState::Connected(_) => break,
                NetworkServiceState::Sending(fut) => {
                    match ready!(fut.poll_unpin(cx)) {
                        // When a send concludes, and there's an error, the request future sends
                        // back `None`. Otherwise, it'll send back `Some(...)` with the socket.
                        Ok(maybe_socket) => match maybe_socket {
                            Some(socket) => NetworkServiceState::Connected(socket),
                            None => NetworkServiceState::Disconnected,
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

        let mut socket = match std::mem::replace(&mut self.state, NetworkServiceState::Sending(rx))
        {
            NetworkServiceState::Connected(socket) => socket,
            _ => panic!("poll_ready must be called first"),
        };

        Box::pin(async move {
            match socket.send(&buf).await.context(net_error::FailedToSend) {
                Ok(sent) => {
                    // Emit an error if we weren't able to send the entire buffer.
                    if sent != buf.len() {
                        socket.on_partial_send(buf.len(), sent);
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
