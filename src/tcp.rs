use serde::{Deserialize, Serialize};

/// Configuration for keepalive probes in a TCP stream.
///
/// This config's properties map to TCP keepalive properties in Tokio/Mio:
/// https://github.com/tokio-rs/mio/blob/e6e403fe2a4fc14dfbc74dbb3ae3a14e3044eb6f/src/net/tcp/socket.rs#L25-L46
///
/// # Note
///
/// Support for the `interval` and `retries` options has just landed in Mio and they are not
/// available in Tokio yet: https://github.com/tokio-rs/tokio/issues/3082. Setting these currently
/// has no effect on a TCP stream in vector. These options are parsed into the options config
/// nevertheless for future support.
///
/// Implementing these options would require upgrading the Tokio runtime accordingly first.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TcpKeepaliveConfig {
    pub time_secs: Option<u64>,
    pub interval_secs: Option<u64>,
    pub retries: Option<u32>,
}
