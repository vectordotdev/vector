use std::{net::SocketAddr, time::Duration};

use snafu::ResultExt;
use tokio::net::{TcpSocket, TcpStream};

use vector_config::configurable_component;
use vector_core::tcp::TcpKeepaliveConfig;

use crate::{dns, net};

use super::{net_error::*, ConnectorType, HostAndPort, NetError, NetworkConnector};

/// TCP configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct TcpConnectorConfig {
    /// The address to connect to.
    ///
    /// Both IP addresses and hostnames/fully-qualified domain names are accepted formats.
    ///
    /// The address _must_ include a port.
    address: HostAndPort,

    #[configurable(derived)]
    keepalive: Option<TcpKeepaliveConfig>,

    /// The size of the socket's send buffer.
    ///
    /// If set, the value of the setting is passed via the `SO_SNDBUF` option.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 65536))]
    send_buffer_size: Option<u32>,
}

impl TcpConnectorConfig {
    pub const fn from_address(host: String, port: u16) -> Self {
        Self {
            address: HostAndPort { host, port },
            keepalive: None,
            send_buffer_size: None,
        }
    }

    pub const fn set_keepalive(mut self, keepalive: TcpKeepaliveConfig) -> Self {
        self.keepalive = Some(keepalive);
        self
    }

    /// Creates a [`NetworkConnector`] from this TCP connector configuration.
    pub fn as_connector(&self) -> NetworkConnector {
        NetworkConnector {
            inner: ConnectorType::Tcp(TcpConnector {
                address: self.address.clone(),
                keepalive: self.keepalive,
                send_buffer_size: self.send_buffer_size,
            }),
        }
    }
}

#[derive(Clone)]
pub(super) struct TcpConnector {
    address: HostAndPort,
    keepalive: Option<TcpKeepaliveConfig>,
    send_buffer_size: Option<u32>,
}

impl TcpConnector {
    pub(super) async fn connect(&self) -> Result<(SocketAddr, TcpStream), NetError> {
        let ip = dns::Resolver
            .lookup_ip(self.address.host.clone())
            .await
            .context(FailedToResolve)?
            .next()
            .ok_or(NetError::NoAddresses)?;

        let addr = SocketAddr::new(ip, self.address.port);

        let socket = if addr.is_ipv4() {
            TcpSocket::new_v4().context(FailedToConfigure)?
        } else {
            TcpSocket::new_v6().context(FailedToConfigure)?
        };

        if let Some(send_buffer_size) = self.send_buffer_size {
            if let Err(error) = socket.set_send_buffer_size(send_buffer_size) {
                warn!(%error, "Failed configuring send buffer size on TCP socket.");
            }
        }

        let stream = socket.connect(addr).await.context(FailedToConnect)?;

        let maybe_keepalive_secs = self
            .keepalive
            .as_ref()
            .and_then(|config| config.time_secs.map(Duration::from_secs));
        if let Some(keepalive_secs) = maybe_keepalive_secs {
            if let Err(error) = net::set_keepalive(&stream, keepalive_secs) {
                warn!(%error, "Failed configuring keepalive on TCP socket.");
            }
        }

        Ok((addr, stream))
    }
}
