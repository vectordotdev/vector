#![deny(missing_docs)]

use super::{EventFinalizer, EventFinalizers, EventStatus};
use getset::{Getters, Setters};
use serde::{Deserialize, Serialize};
use shared::EventDataEq;
use std::sync::Arc;

/// The top-level metadata structure contained by both `struct Metric`
/// and `struct LogEvent` types.
#[derive(
    Clone, Debug, Default, Deserialize, Getters, PartialEq, PartialOrd, Serialize, Setters,
)]
pub struct EventMetadata {
    /// Used to store the datadog API from sources to sinks
    #[getset(get = "pub", set = "pub")]
    #[serde(default, skip)]
    datadog_api_key: Option<Arc<str>>,
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
    /// If a Datadog API key is not set in `self`, the one from `other` will be used.
    pub fn merge(&mut self, other: Self) {
        self.finalizers.merge(other.finalizers);
        if self.datadog_api_key.is_none() {
            self.datadog_api_key = other.datadog_api_key
        }
    }

    /// Update the finalizer(s) status.
    pub fn update_status(&self, status: EventStatus) {
        self.finalizers.update_status(status);
    }

    /// Update the finalizers' sources.
    pub fn update_sources(&mut self) {
        self.finalizers.update_sources();
    }

    /// Add a new finalizer to the array
    pub fn add_finalizer(&mut self, finalizer: EventFinalizer) {
        self.finalizers.add(finalizer);
    }
}

impl EventDataEq for EventMetadata {
    fn event_data_eq(&self, _other: &Self) -> bool {
        // Don't compare the metadata, it is not "event data".
        true
    }
}
