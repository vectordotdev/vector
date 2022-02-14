#![deny(missing_docs)]

use std::sync::Arc;

use getset::{Getters, Setters};
use serde::{Deserialize, Serialize};
use vector_common::EventDataEq;

use super::{BatchNotifier, EventFinalizer, EventFinalizers, EventStatus};
use crate::ByteSizeOf;

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
    /// Used to store the Splunk HEC auth token from sources to sinks
    #[getset(get = "pub", set = "pub")]
    #[serde(default, skip)]
    splunk_hec_token: Option<Arc<str>>,
    #[serde(default, skip)]
    finalizers: EventFinalizers,
}

impl ByteSizeOf for EventMetadata {
    fn allocated_bytes(&self) -> usize {
        // NOTE we don't count the `str` here because it's allocated somewhere
        // else. We're just moving around the pointer, which is already captured
        // by `ByteSizeOf::size_of`.
        self.finalizers.allocated_bytes()
    }
}

impl EventMetadata {
    /// Replace the finalizers array with the given one.
    pub fn with_finalizer(mut self, finalizer: EventFinalizer) -> Self {
        self.finalizers = EventFinalizers::new(finalizer);
        self
    }

    /// Replace the finalizer with a new one created from the given batch notifier.
    pub fn with_batch_notifier(self, batch: &Arc<BatchNotifier>) -> Self {
        self.with_finalizer(EventFinalizer::new(Arc::clone(batch)))
    }

    /// Replace the finalizer with a new one created from the given optional batch notifier.
    pub fn with_batch_notifier_option(self, batch: &Option<Arc<BatchNotifier>>) -> Self {
        match batch {
            Some(batch) => self.with_finalizer(EventFinalizer::new(Arc::clone(batch))),
            None => self,
        }
    }

    /// Merge the other `EventMetadata` into this.
    /// If a Datadog API key is not set in `self`, the one from `other` will be used.
    /// If a Splunk HEC token is not set in `self`, the one from `other` will be used.
    pub fn merge(&mut self, other: Self) {
        self.finalizers.merge(other.finalizers);
        if self.datadog_api_key.is_none() {
            self.datadog_api_key = other.datadog_api_key;
        }
        if self.splunk_hec_token.is_none() {
            self.splunk_hec_token = other.splunk_hec_token;
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

    /// Swap the finalizers list with an empty list and return the original.
    pub fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }

    /// Merges the given finalizers into the existing set of finalizers.
    pub fn merge_finalizers(&mut self, finalizers: EventFinalizers) {
        self.finalizers.merge(finalizers);
    }
}

impl EventDataEq for EventMetadata {
    fn event_data_eq(&self, _other: &Self) -> bool {
        // Don't compare the metadata, it is not "event data".
        true
    }
}

/// This is a simple wrapper to allow attaching `EventMetadata` to any
/// other type. This is primarily used in conversion functions, such as
/// `impl From<X> for WithMetadata<Y>`.
pub struct WithMetadata<T> {
    /// The data item being wrapped.
    pub data: T,
    /// The additional metadata sidecar.
    pub metadata: EventMetadata,
}

impl<T> WithMetadata<T> {
    /// Convert from one wrapped type to another, where the underlying
    /// type allows direct conversion.
    // We would like to `impl From` instead, but this fails due to
    // conflicting implementations of `impl<T> From<T> for T`.
    pub fn into<T1: From<T>>(self) -> WithMetadata<T1> {
        WithMetadata {
            data: T1::from(self.data),
            metadata: self.metadata,
        }
    }
}
