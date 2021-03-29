pub mod logs;
pub mod metrics;

use crate::{event::LookupBuf, sinks::splunk_hec};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Json,
    Text,
}

impl From<Encoding> for splunk_hec::Encoding {
    fn from(v: Encoding) -> Self {
        match v {
            Encoding::Json => splunk_hec::Encoding::Json,
            Encoding::Text => splunk_hec::Encoding::Text,
        }
    }
}

fn host_key() -> LookupBuf {
    crate::config::log_schema().host_key().clone()
}
