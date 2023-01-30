use std::{
    io,
    os::fd::{AsRawFd, RawFd},
    path::{Path, PathBuf},
    task::{ready, Context, Poll},
    time::Duration,
};

use futures::{future::BoxFuture, FutureExt};
use snafu::{ResultExt, Snafu};
use tokio::{
    io::AsyncWriteExt,
    net::{UnixDatagram, UnixStream},
    sync::oneshot,
    time::sleep,
};
use tower::Service;

use vector_config::configurable_component;

use crate::{
    internal_events::{
        UnixSendIncompleteError, UnixSocketConnectionEstablished, UnixSocketOutgoingConnectionError,
    },
    net,
    sinks::{util::retries::ExponentialBackoff, Healthcheck},
};

#[derive(Debug, Snafu)]
pub enum UnixError {
    #[snafu(display("Failed to bind Unix socket: {}.", source))]
    FailedToBind { source: std::io::Error },

    #[snafu(display("Failed to send Unix datagram: {}", source))]
    FailedToSend { source: std::io::Error },

    #[snafu(display("Failed to connect to Unix endpoint: {}", source))]
    FailedToConnect { source: std::io::Error },

    #[snafu(display("Failed to get Unix socket back after send as channel closed unexpectedly."))]
    ServiceSocketChannelClosed,
}

/// Unix socket modes.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
pub enum UnixMode {
    /// Datagram-oriented mode.
    ///
    /// This corresponds to the socket having the `SOCK_DGRAM` type.
    Datagram,

    /// Stream-oriented mode.
    ///
    /// This corresponds to the socket having the `SOCK_STREAM` type.
    Stream,
}

/// `UnixConnector` configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UnixConnectorConfig {
    /// The Unix socket path.
    ///
    /// This should be an absolute path.
    path: PathBuf,

    /// The Unix socket mode to use.
    unix_mode: UnixMode,

    /// The size of the socket's send buffer, in bytes.
    ///
    /// If set, the value of the setting is passed via the `SO_SNDBUF` option.
    send_buffer_size: Option<usize>,
}

impl UnixConnectorConfig {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            unix_mode: UnixMode::Stream,
            send_buffer_size: None,
        }
    }

    pub const fn set_unix_mode(mut self, unix_mode: UnixMode) -> Self {
        self.unix_mode = unix_mode;
        self
    }

    pub fn as_connector(&self) -> UnixConnector {
        UnixConnector {
            path: self.path.clone(),
            mode: self.unix_mode,
            send_buffer_size: self.send_buffer_size,
        }
    }
}

enum UnixEither {
    Datagram(UnixDatagram),
    Stream(UnixStream),
}

impl UnixEither {
    pub async fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Datagram(datagram) => datagram.send(buf).await,
            Self::Stream(stream) => stream.write_all(buf).await.map(|_| buf.len()),
        }
    }
}

impl AsRawFd for UnixEither {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Self::Datagram(datagram) => datagram.as_raw_fd(),
            Self::Stream(stream) => stream.as_raw_fd(),
        }
    }
}

#[derive(Clone)]
pub struct UnixConnector {
    path: PathBuf,
    mode: UnixMode,
    send_buffer_size: Option<usize>,
}

impl UnixConnector {
    async fn connect(&self) -> Result<(PathBuf, UnixEither), UnixError> {
        let either_socket = match self.mode {
            UnixMode::Datagram => UnixDatagram::unbound()
                .context(FailedToBindSnafu)
                .and_then(|datagram| {
                    datagram
                        .connect(&self.path)
                        .context(FailedToConnectSnafu)
                        .map(|_| UnixEither::Datagram(datagram))
                })?,
            UnixMode::Stream => UnixStream::connect(&self.path)
                .await
                .context(FailedToConnectSnafu)
                .map(UnixEither::Stream)?,
        };

        if let Some(send_buffer_size) = self.send_buffer_size {
            if let Err(error) = net::set_send_buffer_size(&either_socket, send_buffer_size) {
                warn!(%error, "Failed configuring send buffer size on Unix socket.");
            }
        }

        Ok((self.path.clone(), either_socket))
    }

    async fn connect_backoff(&self) -> UnixEither {
        // TODO: Make this configurable.
        let mut backoff = ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60));

        loop {
            match self.connect().await {
                Ok((path, either_socket)) => {
                    emit!(UnixSocketConnectionEstablished { path: &path });
                    return either_socket;
                }
                Err(error) => {
                    emit!(UnixSocketOutgoingConnectionError { error });
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
    pub fn service(&self) -> UnixService {
        UnixService::new(self.clone())
    }
}

enum UnixServiceState {
    /// The service is currently disconnected.
    Disconnected,

    /// The service is currently attempting to connect to the endpoint.
    Connecting(BoxFuture<'static, UnixEither>),

    /// The service is connected and idle.
    Connected(UnixEither),

    /// The service has an in-flight send to the socket.
    ///
    /// If the socket experiences an unrecoverable error during the send, `None` will be returned
    /// over the channel to signal the need to establish a new connection rather than reusing the
    /// existing connection.
    Sending(oneshot::Receiver<Option<UnixEither>>),
}

pub struct UnixService {
    connector: UnixConnector,
    state: UnixServiceState,
}

impl UnixService {
    const fn new(connector: UnixConnector) -> Self {
        Self {
            connector,
            state: UnixServiceState::Disconnected,
        }
    }
}

impl Service<Vec<u8>> for UnixService {
    type Response = usize;
    type Error = UnixError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.state = match &mut self.state {
                UnixServiceState::Disconnected => {
                    let connector = self.connector.clone();
                    UnixServiceState::Connecting(Box::pin(async move {
                        connector.connect_backoff().await
                    }))
                }
                UnixServiceState::Connecting(fut) => {
                    let socket = ready!(fut.poll_unpin(cx));
                    UnixServiceState::Connected(socket)
                }
                UnixServiceState::Connected(_) => break,
                UnixServiceState::Sending(fut) => {
                    match ready!(fut.poll_unpin(cx)) {
                        // When a send concludes, and there's an error, the request future sends
                        // back `None`. Otherwise, it'll send back `Some(...)` with the socket.
                        Ok(maybe_socket) => match maybe_socket {
                            Some(socket) => UnixServiceState::Connected(socket),
                            None => UnixServiceState::Disconnected,
                        },
                        Err(_) => return Poll::Ready(Err(UnixError::ServiceSocketChannelClosed)),
                    }
                }
            };
        }
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, buf: Vec<u8>) -> Self::Future {
        let (tx, rx) = oneshot::channel();

        let mut socket = match std::mem::replace(&mut self.state, UnixServiceState::Sending(rx)) {
            UnixServiceState::Connected(socket) => socket,
            _ => panic!("poll_ready must be called first"),
        };

        Box::pin(async move {
            match socket.send(&buf).await.context(FailedToSendSnafu) {
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
