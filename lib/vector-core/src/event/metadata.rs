#![deny(missing_docs)]

use super::{EventFinalizer, EventFinalizers, EventStatus};
use serde::{Deserialize, Serialize};
use shared::EventDataEq;

/// The top-level metadata structure contained by both `struct Metric`
/// and `struct LogEvent` types.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EventMetadata {
    #[serde(default, skip)]
    finalizers: EventFinalizers,
}

impl EventMetadata {
    /// Replace the finalizers array with the given one.
    pub fn with_finalizer(mut self, finalizer: EventFinalizer) -> Self {
        self.finalizers = EventFinalizers::new(finalizer);
        self
    }

    /// Merge the other `EventMetadata` into this.
    pub fn merge(&mut self, other: Self) {
        self.finalizers.merge(other.finalizers);
    }

    /// Update the finalizer(s) status.
    pub fn update_status(&self, status: EventStatus) {
        self.finalizers.update_status(status);
    }

    /// Update the finalizers' sources.
    pub fn update_sources(&mut self) {
        self.finalizers.update_sources();
    }
}

impl EventDataEq for EventMetadata {
    fn event_data_eq(&self, _other: &Self) -> bool {
        // Don't compare the metadata, it is not "event data".
        true
    }
}
