use serde::{Deserialize, Serialize};
use std::time::Duration;

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
