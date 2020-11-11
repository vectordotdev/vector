use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TcpKeepaliveConfig {
    pub time: Option<Duration>,
    // pub interval: Option<Duration>,
    // pub retries: Option<u32>,
}
