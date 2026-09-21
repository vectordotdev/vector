//! Canonical typed trace event (RFC 25329).
//!
//! This module is the internal typed model. It is not re-exported as
//! `vector_core::event::TraceEvent`; that name remains the legacy `LogEvent` newtype.
//!
//! Gated behind the `typed-trace` cargo feature until a source, transform, or sink
//! consumes it. The module also compiles under `cfg(test)` so this crate's unit tests
//! run without enabling the feature for dependents.

mod attributes;
mod datadog;
mod enums;
mod flags;
mod ids;
mod span;
#[cfg(test)]
mod tests;

pub use attributes::{AttrMap, AttrValue, Attributes};
pub use datadog::{
    DatadogAgentEnvelope, DatadogChunkContext, DatadogEventContext, DatadogSpanContext,
    DatadogTracerContext,
};
pub use enums::{SamplingPriority, SpanKind, SpanStatus};
pub use flags::{TraceFlags, TraceState};
pub use ids::{InvalidIdError, SpanId, TraceId};
pub use span::{DroppedCount, Resource, Scope, Span, SpanEvent, SpanLink};

use vector_buffers::EventCount;
use vector_common::{
    EventDataEq,
    byte_size_of::ByteSizeOf,
    internal_event::{OptionalTag, TaggedEventsSent},
    json_size::JsonSize,
    request_metadata::GetEventCountTags,
};

use super::{
    BatchNotifier, EstimatedJsonEncodedSizeOf, EventFinalizer, EventFinalizers, EventMetadata,
    Finalizable, MergeFinalizable,
};
use crate::config::telemetry;

/// Trace-homogeneous typed event: one [`TraceId`], one [`Resource`], one [`Scope`],
/// Datadog-native context, and the spans belonging to that grouping.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceEvent {
    trace_id: TraceId,
    resource: Resource,
    scope: Scope,
    datadog: DatadogEventContext,
    spans: Vec<Span>,
    metadata: EventMetadata,
}

impl TraceEvent {
    /// Creates an event with the given metadata and default resource, scope, and Datadog context.
    #[must_use]
    pub fn new_with_metadata(trace_id: TraceId, metadata: EventMetadata) -> Self {
        Self {
            trace_id,
            resource: Resource::default(),
            scope: Scope::default(),
            datadog: DatadogEventContext::default(),
            spans: Vec::new(),
            metadata,
        }
    }

    /// Creates an empty event for `trace_id`.
    ///
    /// An empty span list is representable and retains the event-level ID.
    #[must_use]
    pub fn new(trace_id: TraceId) -> Self {
        Self::new_with_metadata(trace_id, EventMetadata::default())
    }

    /// Event-level trace ID. Contained spans have no duplicate of this field.
    #[must_use]
    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    /// Reassigns the event-level trace ID without inspecting spans.
    pub const fn set_trace_id(&mut self, trace_id: TraceId) {
        self.trace_id = trace_id;
    }

    /// Resource attached to this event.
    #[must_use]
    pub const fn resource(&self) -> &Resource {
        &self.resource
    }

    /// Mutable resource.
    pub const fn resource_mut(&mut self) -> &mut Resource {
        &mut self.resource
    }

    /// Instrumentation scope attached to this event.
    #[must_use]
    pub const fn scope(&self) -> &Scope {
        &self.scope
    }

    /// Mutable instrumentation scope.
    pub const fn scope_mut(&mut self) -> &mut Scope {
        &mut self.scope
    }

    /// Datadog-native event context. Default means none is present.
    #[must_use]
    pub const fn datadog(&self) -> &DatadogEventContext {
        &self.datadog
    }

    /// Mutable Datadog-native event context.
    pub const fn datadog_mut(&mut self) -> &mut DatadogEventContext {
        &mut self.datadog
    }

    /// Spans belonging to [`Self::trace_id`] within this resource/scope grouping.
    #[must_use]
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Mutable span list. Insertion does not rewrite span identity.
    pub fn spans_mut(&mut self) -> &mut Vec<Span> {
        &mut self.spans
    }

    /// Event metadata.
    #[must_use]
    pub const fn metadata(&self) -> &EventMetadata {
        &self.metadata
    }

    /// Mutable event metadata.
    pub const fn metadata_mut(&mut self) -> &mut EventMetadata {
        &mut self.metadata
    }

    /// Adds a finalizer to this event.
    pub fn add_finalizer(&mut self, finalizer: EventFinalizer) {
        self.metadata.add_finalizer(finalizer);
    }

    /// Returns this event with finalizers attached to `batch`.
    #[must_use]
    pub fn with_batch_notifier(mut self, batch: &BatchNotifier) -> Self {
        self.metadata = self.metadata.with_batch_notifier(batch);
        self
    }

    /// Returns this event with optional finalizers attached to `batch`.
    #[must_use]
    pub fn with_batch_notifier_option(mut self, batch: &Option<BatchNotifier>) -> Self {
        self.metadata = self.metadata.with_batch_notifier_option(batch);
        self
    }
}

impl ByteSizeOf for TraceEvent {
    fn allocated_bytes(&self) -> usize {
        self.resource.allocated_bytes()
            + self.scope.allocated_bytes()
            + self.datadog.allocated_bytes()
            + self.spans.allocated_bytes()
            + self.metadata.allocated_bytes()
    }
}

impl EstimatedJsonEncodedSizeOf for TraceEvent {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        // Typed JSON encoding is not in this stage; use the in-memory size as Metric does.
        self.size_of().into()
    }
}

impl EventCount for TraceEvent {
    fn event_count(&self) -> usize {
        1
    }
}

impl EventDataEq for TraceEvent {
    fn event_data_eq(&self, other: &Self) -> bool {
        self.trace_id == other.trace_id
            && self.resource == other.resource
            && self.scope == other.scope
            && self.datadog == other.datadog
            && self.spans == other.spans
            && self.metadata.event_data_eq(&other.metadata)
    }
}

impl Finalizable for TraceEvent {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.metadata.take_finalizers()
    }
}

impl MergeFinalizable for TraceEvent {
    fn merge_finalizers(&mut self, finalizers: EventFinalizers) {
        self.metadata.merge_finalizers(finalizers);
    }
}

impl GetEventCountTags for TraceEvent {
    fn get_tags(&self) -> TaggedEventsSent {
        let source = if telemetry().tags().emit_source {
            self.metadata.source_id().cloned().into()
        } else {
            OptionalTag::Ignored
        };

        let service = if telemetry().tags().emit_service {
            self.resource.service.clone().into()
        } else {
            OptionalTag::Ignored
        };

        TaggedEventsSent { source, service }
    }
}

// `EventMetadata` is `PartialEq` only (`Value`, ignored transform timestamp), so this
// cannot be derived. The comparison is still an equivalence relation on the fields
// [`PartialEq`] uses.
impl Eq for TraceEvent {}
