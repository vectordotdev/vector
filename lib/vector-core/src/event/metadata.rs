#![deny(missing_docs)]

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use vector_common::EventDataEq;

use super::{BatchNotifier, EventFinalizer, EventFinalizers, EventStatus};
use crate::{schema, ByteSizeOf};

/// The top-level metadata structure contained by both `struct Metric`
/// and `struct LogEvent` types.
#[derive(Clone, Debug, Deserialize, PartialEq, PartialOrd, Serialize)]
pub struct EventMetadata {
    /// Used to store the datadog API from sources to sinks
    #[serde(default, skip)]
    datadog_api_key: Option<Arc<str>>,
    /// Used to store the Splunk HEC auth token from sources to sinks
    #[serde(default, skip)]
    splunk_hec_token: Option<Arc<str>>,
    #[serde(default, skip)]
    finalizers: EventFinalizers,

    /// An identifier for a globaly registered schema definition which provides information about
    /// the event shape (type information, and semantic meaning of fields).
    ///
    /// TODO(Jean): must not skip serialization to track schemas across restarts.
    #[serde(default = "default_schema_definition", skip)]
    schema_definition: Arc<schema::Definition>,
}

impl EventMetadata {
    /// Return the datadog API key, if it exists
    pub fn datadog_api_key(&self) -> &Option<Arc<str>> {
        &self.datadog_api_key
    }

    /// Set the datadog API key to passed value
    pub fn set_datadog_api_key(&mut self, key: Option<Arc<str>>) {
        self.datadog_api_key = key;
    }

    /// Return the splunk hec token, if it exists
    pub fn splunk_hec_token(&self) -> &Option<Arc<str>> {
        &self.splunk_hec_token
    }

    /// Set the splunk hec token to passed value
    pub fn set_splunk_hec_token(&mut self, token: Option<Arc<str>>) {
        self.splunk_hec_token = token;
    }
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self {
            datadog_api_key: Default::default(),
            splunk_hec_token: Default::default(),
            finalizers: Default::default(),
            schema_definition: default_schema_definition(),
        }
    }
}

fn default_schema_definition() -> Arc<schema::Definition> {
    Arc::new(schema::Definition::empty())
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
    /// Replaces the existing event finalizers with the given one.
    #[must_use]
    pub fn with_finalizer(mut self, finalizer: EventFinalizer) -> Self {
        self.finalizers = EventFinalizers::new(finalizer);
        self
    }

    /// Replaces the existing event finalizers with the given ones.
    #[must_use]
    pub fn with_finalizers(mut self, finalizers: EventFinalizers) -> Self {
        self.finalizers = finalizers;
        self
    }

    /// Replace the finalizer with a new one created from the given batch notifier.
    #[must_use]
    pub fn with_batch_notifier(self, batch: &Arc<BatchNotifier>) -> Self {
        self.with_finalizer(EventFinalizer::new(Arc::clone(batch)))
    }

    /// Replace the finalizer with a new one created from the given optional batch notifier.
    #[must_use]
    pub fn with_batch_notifier_option(self, batch: &Option<Arc<BatchNotifier>>) -> Self {
        match batch {
            Some(batch) => self.with_finalizer(EventFinalizer::new(Arc::clone(batch))),
            None => self,
        }
    }

    /// Replace the schema definition with the given one.
    #[must_use]
    pub fn with_schema_definition(mut self, schema_definition: &Arc<schema::Definition>) -> Self {
        self.schema_definition = Arc::clone(schema_definition);
        self
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

    /// Gets a reference to the event finalizers.
    pub fn finalizers(&self) -> &EventFinalizers {
        &self.finalizers
    }

    /// Add a new event finalizer to the existing list of event finalizers.
    pub fn add_finalizer(&mut self, finalizer: EventFinalizer) {
        self.finalizers.add(finalizer);
    }

    /// Consumes all event finalizers and returns them, leaving the list of event finalizers empty.
    pub fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }

    /// Merges the given event finalizers into the existing list of event finalizers.
    pub fn merge_finalizers(&mut self, finalizers: EventFinalizers) {
        self.finalizers.merge(finalizers);
    }

    /// Get the schema definition.
    pub fn schema_definition(&self) -> &schema::Definition {
        self.schema_definition.as_ref()
    }

    /// Set the schema definition.
    pub fn set_schema_definition(&mut self, definition: &Arc<schema::Definition>) {
        self.schema_definition = Arc::clone(definition);
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
