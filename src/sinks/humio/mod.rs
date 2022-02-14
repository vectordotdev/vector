pub mod logs;
pub mod metrics;

use serde::{Deserialize, Serialize};

use crate::sinks::splunk_hec;

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Json,
    Text,
}

impl From<Encoding> for splunk_hec::logs::encoder::HecLogsEncoder {
    fn from(v: Encoding) -> Self {
        match v {
            Encoding::Json => splunk_hec::logs::encoder::HecLogsEncoder::Json,
            Encoding::Text => splunk_hec::logs::encoder::HecLogsEncoder::Text,
        }
    }
}

fn host_key() -> String {
    crate::config::log_schema().host_key().to_string()
}
