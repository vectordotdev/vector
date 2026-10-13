use std::path::PathBuf;

use super::{
    ConnectorType, NetError, NetworkConnector, UnixConnectorConfig, UnixMode, net_error::*,
};
use crate::{net, sinks::util::unix::UnixEither};
use snafu::ResultExt;
use tokio::net::{UnixDatagram, UnixStream};

impl UnixConnectorConfig {
    /// Creates a [`NetworkConnector`] from this Unix Domain Socket connector configuration.
    pub fn as_connector(&self) -> Result<NetworkConnector, NetError> {
        Ok(NetworkConnector {
            inner: ConnectorType::Unix(UnixConnector {
                path: self.path.clone(),
                mode: self.unix_mode,
                send_buffer_size: self.send_buffer_size,
            }),
        })
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

        if let Some(send_buffer_size) = self.send_buffer_size
            && let Err(error) = net::set_send_buffer_size(&either_socket, send_buffer_size)
        {
            warn!(%error, "Failed configuring send buffer size on Unix socket.");
        }

        Ok((self.path.clone(), either_socket))
    }
}
