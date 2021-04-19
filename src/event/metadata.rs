#![deny(missing_docs)]

use super::{EventFinalizer, MaybeEventFinalizer};
use serde::{Deserialize, Serialize};
use shared::EventDataEq;

/// The top-level metadata structure contained by both `struct Metric`
/// and `struct LogEvent` types.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EventMetadata {
    #[serde(default, skip)]
    finalizer: MaybeEventFinalizer,
}

impl EventMetadata {
    /// Replace the finalizers array with the given one.
    pub fn with_finalizer(self, finalizer: EventFinalizer) -> Self {
        Self {
            finalizer: finalizer.into(),
        }
    }

    /// Merge the other `EventMetadata` into this.
    pub fn merge(&mut self, other: Self) {
        self.finalizer.merge(other.finalizer);
    }
}

impl EventDataEq for EventMetadata {
    fn event_data_eq(&self, _other: &Self) -> bool {
        // Don't compare the metadata, it is not "event data".
        true
    }
}
