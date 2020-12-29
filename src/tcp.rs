use serde::{Deserialize, Serialize};

/// Configuration for keepalive probes in a TCP stream.
///
/// This config's properties map to TCP keepalive properties in Tokio:
/// https://github.com/tokio-rs/tokio/blob/tokio-0.2.22/tokio/src/net/tcp/stream.rs#L516-L537
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TcpKeepaliveConfig {
    pub time_secs: Option<u64>,
}
