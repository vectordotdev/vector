#![deny(missing_docs)]

use super::{EventFinalizer, EventFinalizers};
use serde::{Deserialize, Serialize};
use shared::EventDataEq;

/// The top-level metadata structure contained by both `struct Metric`
/// and `struct LogEvent` types.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EventMetadata {
    #[serde(default, skip)]
    finalizers: Option<EventFinalizers>,
}

impl EventMetadata {
    /// Replace the finalizers array with the given one.
    pub fn with_finalizer(self, finalizer: EventFinalizer) -> Self {
        Self {
            finalizers: Some(EventFinalizers::new(finalizer)),
        }
    }

    /// Merge the other `EventMetadata` into this.
    pub fn merge(&mut self, other: Self) {
        self.finalizers = match (self.finalizers.take(), other.finalizers) {
            (None, None) => None,
            (Some(f), None) => Some(f),
            (None, Some(f)) => Some(f),
            (Some(mut f1), Some(f2)) => {
                f1.merge(f2);
                Some(f1)
            }
        };
    }
}

impl EventDataEq for EventMetadata {
    fn event_data_eq(&self, _other: &Self) -> bool {
        // Don't compare the metadata, it is not "event data".
        true
    }
}
