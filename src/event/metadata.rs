use serde::{Deserialize, Serialize};
use shared::EventDataEq;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EventMetadata;

impl EventDataEq for EventMetadata {
    fn event_data_eq(&self, _other: &Self) -> bool {
        true
    }
}
