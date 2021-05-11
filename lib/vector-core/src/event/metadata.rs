#![deny(missing_docs)]

use super::{EventFinalizer, EventFinalizers, EventStatus};
use getset::Getters;
use serde::{Deserialize, Serialize};
use shared::EventDataEq;

/// The top-level metadata structure contained by both `struct Metric`
/// and `struct LogEvent` types.
#[derive(Clone, Debug, Default, Deserialize, Getters, PartialEq, PartialOrd, Serialize)]
pub struct EventMetadata {
    #[serde(default, skip)]
    /// Used to store the datadog API from sources to sinks
    #[get = "pub"]
    datadog_api_key: Option<String>,
    #[serde(default, skip)]
    finalizers: EventFinalizers,
}

impl EventMetadata {
    /// Build metadata with datadog api key only
    pub fn with_datadog_api_key(api_key: String) -> Self {
        Self {
            datadog_api_key: Some(api_key),
            finalizers: EventFinalizers::default(),
        }
    }

    /// Replace the finalizers array with the given one.
    pub fn with_finalizer(mut self, finalizer: EventFinalizer) -> Self {
        self.finalizers = EventFinalizers::new(finalizer);
        self
    }

    /// Merge the other `EventMetadata` into this.
    pub fn merge(&mut self, other: Self) {
        self.finalizers.merge(other.finalizers);
        match (
            self.datadog_api_key.as_ref(),
            other.datadog_api_key.as_ref(),
        ) {
            (None, Some(_)) => self.datadog_api_key = other.datadog_api_key.clone(),
            (_, _) => (),
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
