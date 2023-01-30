mod tcp;
mod udp;
mod unix;

pub use self::tcp::{TcpConnector, TcpConnectorConfig};
pub use self::udp::{UdpConnector, UdpConnectorConfig};
pub use self::unix::{UnixConnector, UnixConnectorConfig, UnixMode};

use vector_config::configurable_component;

/// Hostname and port tuple.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(try_from = "String", into = "String")]
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
