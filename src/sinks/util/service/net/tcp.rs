use std::net::SocketAddr;

use snafu::ResultExt;
use tokio::net::TcpStream;

use vector_lib::configurable::configurable_component;
use vector_lib::{
    tcp::TcpKeepaliveConfig,
    tls::{MaybeTlsSettings, MaybeTlsStream, TlsEnableableConfig},
};

use crate::dns;

use super::{net_error::*, ConnectorType, HostAndPort, NetError, NetworkConnector};

/// TCP configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct TcpConnectorConfig {
    #[configurable(derived)]
    address: HostAndPort,

    #[configurable(derived)]
    keepalive: Option<TcpKeepaliveConfig>,

    /// The size of the socket's send buffer.
    ///
    /// If set, the value of the setting is passed via the `SO_SNDBUF` option.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 65536))]
    send_buffer_size: Option<usize>,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,
}

impl TcpConnectorConfig {
    pub const fn from_address(host: String, port: u16) -> Self {
        Self {
            address: HostAndPort { host, port },
            keepalive: None,
            send_buffer_size: None,
            tls: None,
        }
    }

    /// Creates a [`NetworkConnector`] from this TCP connector configuration.
    pub fn as_connector(&self) -> NetworkConnector {
        NetworkConnector {
            inner: ConnectorType::Tcp(TcpConnector {
                address: self.address.clone(),
                keepalive: self.keepalive,
                send_buffer_size: self.send_buffer_size,
                tls: self.tls.clone(),
            }),
        }
    }
}

#[derive(Clone)]
pub(super) struct TcpConnector {
    address: HostAndPort,
    keepalive: Option<TcpKeepaliveConfig>,
    send_buffer_size: Option<usize>,
    tls: Option<TlsEnableableConfig>,
}

impl TcpConnector {
    pub(super) async fn connect(
        &self,
    ) -> Result<(SocketAddr, MaybeTlsStream<TcpStream>), NetError> {
        let ip = dns::Resolver
            .lookup_ip(self.address.host.clone())
            .await
            .context(FailedToResolve)?
            .next()
            .ok_or(NetError::NoAddresses)?;

        let addr = SocketAddr::new(ip, self.address.port);

        let tls = MaybeTlsSettings::from_config(&self.tls, false).context(FailedToConfigureTLS)?;
        let mut stream = tls
            .connect(self.address.host.as_str(), &addr)
            .await
            .context(FailedToConnectTLS)?;

        if let Some(send_buffer_size) = self.send_buffer_size {
            if let Err(error) = stream.set_send_buffer_bytes(send_buffer_size) {
                warn!(%error, "Failed configuring send buffer size on TCP socket.");
            }
        }

        if let Some(keepalive) = self.keepalive {
            if let Err(error) = stream.set_keepalive(keepalive) {
                warn!(%error, "Failed configuring keepalive on TCP socket.");
            }
        }

        Ok((addr, stream))
    }
}
