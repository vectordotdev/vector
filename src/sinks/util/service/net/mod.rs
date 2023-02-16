mod tcp;
mod udp;
mod unix;

pub use self::tcp::{TcpConnector, TcpConnectorConfig};
pub use self::udp::{UdpConnector, UdpConnectorConfig};
pub use self::unix::{UnixConnector, UnixConnectorConfig, UnixMode};

use snafu::Snafu;
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
