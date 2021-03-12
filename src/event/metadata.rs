use super::Equivalent;
use chrono::{DateTime, Utc};
use getset::CopyGetters;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, CopyGetters, Eq, PartialEq, Serialize)]
pub struct EventMetadata {
    #[getset(get_copy = "pub")]
    timestamp: DateTime<Utc>,
}

impl EventMetadata {
    pub fn now() -> Self {
        Self {
            timestamp: Utc::now(),
        }
    }

    pub fn with_timestamp(timestamp: DateTime<Utc>) -> Self {
        Self { timestamp }
    }

    pub fn merge(&mut self, other: &Self) {
        // Set the timestamp to the earliest to ensure we track the
        // first time a source event entered our system when two events
        // are merged.
        self.timestamp = self.timestamp.min(other.timestamp);
    }
}

impl Equivalent for EventMetadata {
    fn equivalent(&self, _other: &Self) -> bool {
        true
    }
}
