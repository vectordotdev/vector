use serde::{Deserialize, Serialize};
use std::time::Duration;

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
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "duration")]
    pub time: Option<Duration>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "duration")]
    pub interval: Option<Duration>,
    pub retries: Option<u32>,
}

mod duration {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = duration.unwrap();
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let time: u64 = Deserialize::deserialize(deserializer)?;
        Ok(Some(Duration::from_secs(time)))
    }
}
