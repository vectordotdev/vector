use serde::{Deserialize, Serialize};
use shared::EventDataEq;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EventMetadata {
    //finalizer: Option<Arc<EventFinalizer>>,
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self {} //finalizer: None }
    }
}

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
