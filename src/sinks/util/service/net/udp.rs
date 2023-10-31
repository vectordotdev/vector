use std::net::SocketAddr;

use snafu::ResultExt;
use tokio::net::UdpSocket;

use vector_lib::configurable::configurable_component;

use crate::{dns, net};

use super::{net_error::*, ConnectorType, HostAndPort, NetError, NetworkConnector};

/// UDP configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UdpConnectorConfig {
    #[configurable(derived)]
    address: HostAndPort,

    /// The size of the socket's send buffer.
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

    /// Creates a [`NetworkConnector`] from this UDP connector configuration.
    pub fn as_connector(&self) -> NetworkConnector {
        NetworkConnector {
            inner: ConnectorType::Udp(UdpConnector {
                address: self.address.clone(),
                send_buffer_size: self.send_buffer_size,
            }),
        }
    }
}

#[derive(Clone)]
pub(super) struct UdpConnector {
    address: HostAndPort,
    send_buffer_size: Option<usize>,
}

impl UdpConnector {
    pub(super) async fn connect(&self) -> Result<UdpSocket, NetError> {
        let ip = dns::Resolver
            .lookup_ip(self.address.host.clone())
            .await
            .context(FailedToResolve)?
            .next()
            .ok_or(NetError::NoAddresses)?;

        let addr = SocketAddr::new(ip, self.address.port);
        let bind_address = crate::sinks::util::udp::find_bind_address(&addr);

        let socket = UdpSocket::bind(bind_address).await.context(FailedToBind)?;

        if let Some(send_buffer_size) = self.send_buffer_size {
            if let Err(error) = net::set_send_buffer_size(&socket, send_buffer_size) {
                warn!(%error, "Failed configuring send buffer size on UDP socket.");
            }
        }

        socket.connect(addr).await.context(FailedToConnect)?;

        Ok(socket)
    }
}
