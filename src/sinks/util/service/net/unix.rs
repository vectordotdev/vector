use std::{
    io,
    os::fd::{AsFd, BorrowedFd},
    path::{Path, PathBuf},
};

use snafu::ResultExt;
use tokio::{
    io::AsyncWriteExt,
    net::{UnixDatagram, UnixStream},
};

use vector_lib::configurable::configurable_component;

use crate::net;

use super::{net_error::*, ConnectorType, NetError, NetworkConnector};

/// Unix socket modes.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
pub enum UnixMode {
    /// Datagram-oriented (`SOCK_DGRAM`).
    Datagram,

    /// Stream-oriented (`SOCK_STREAM`).
    Stream,
}

/// Unix Domain Socket configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UnixConnectorConfig {
    /// The Unix socket path.
    ///
    /// This should be an absolute path.
    #[configurable(metadata(docs::examples = "/path/to/socket"))]
    path: PathBuf,

    /// The Unix socket mode to use.
    #[serde(default = "default_unix_mode")]
    unix_mode: UnixMode,

    /// The size of the socket's send buffer.
    ///
    /// If set, the value of the setting is passed via the `SO_SNDBUF` option.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 65536))]
    send_buffer_size: Option<usize>,
}

const fn default_unix_mode() -> UnixMode {
    UnixMode::Stream
}

impl UnixConnectorConfig {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            unix_mode: UnixMode::Stream,
            send_buffer_size: None,
        }
    }

    /// Creates a [`NetworkConnector`] from this Unix Domain Socket connector configuration.
    pub fn as_connector(&self) -> NetworkConnector {
        NetworkConnector {
            inner: ConnectorType::Unix(UnixConnector {
                path: self.path.clone(),
                mode: self.unix_mode,
                send_buffer_size: self.send_buffer_size,
            }),
        }
    }
}

pub(super) enum UnixEither {
    Datagram(UnixDatagram),
    Stream(UnixStream),
}

impl UnixEither {
    pub(super) async fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Datagram(datagram) => datagram.send(buf).await,
            Self::Stream(stream) => stream.write_all(buf).await.map(|_| buf.len()),
        }
    }
}

impl AsFd for UnixEither {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match self {
            Self::Datagram(datagram) => datagram.as_fd(),
            Self::Stream(stream) => stream.as_fd(),
        }
    }
}

#[derive(Clone)]
pub(super) struct UnixConnector {
    path: PathBuf,
    mode: UnixMode,
    send_buffer_size: Option<usize>,
}

impl UnixConnector {
    pub(super) async fn connect(&self) -> Result<(PathBuf, UnixEither), NetError> {
        let either_socket = match self.mode {
            UnixMode::Datagram => {
                UnixDatagram::unbound()
                    .context(FailedToBind)
                    .and_then(|datagram| {
                        datagram
                            .connect(&self.path)
                            .context(FailedToConnect)
                            .map(|_| UnixEither::Datagram(datagram))
                    })?
            }
            UnixMode::Stream => UnixStream::connect(&self.path)
                .await
                .context(FailedToConnect)
                .map(UnixEither::Stream)?,
        };

        if let Some(send_buffer_size) = self.send_buffer_size {
            if let Err(error) = net::set_send_buffer_size(&either_socket, send_buffer_size) {
                warn!(%error, "Failed configuring send buffer size on Unix socket.");
            }
        }

        Ok((self.path.clone(), either_socket))
    }
}
