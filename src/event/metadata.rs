use serde::{Deserialize, Serialize};
use shared::EventDataEq;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EventMetadata;

impl EventMetadata {
    pub fn merge(&mut self, _other: &Self) {
        // Just a stub function for when there is actual metadata
    }
}

impl EventDataEq for EventMetadata {
    fn event_data_eq(&self, _other: &Self) -> bool {
        true
    }
}
