//! Span, resource, scope, events, and links.

use std::time::Duration;

use chrono::{DateTime, Utc};
use vector_common::byte_size_of::ByteSizeOf;

use super::{
    Attributes, DatadogSpanContext, SpanId, SpanKind, SpanStatus, TraceFlags, TraceId, TraceState,
};

/// In-band dropped-item count (`dropped_*_count` on OTLP and this model).
///
/// Mapping- and relay-generated increments saturate. Explicit assignment (VRL, constructors)
/// should use [`Self::new`] / [`From<u32>`] rather than saturating arithmetic.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DroppedCount(u32);

impl DroppedCount {
    /// Zero dropped items.
    pub const ZERO: Self = Self(0);
    /// Maximum representable count; further saturating adds stay here.
    pub const MAX: Self = Self(u32::MAX);

    /// Constructs a count from a `u32`.
    #[must_use]
    pub const fn new(n: u32) -> Self {
        Self(n)
    }

    /// Returns the integer value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Saturating increment for mapping-generated drops.
    pub const fn increment(&mut self, n: u32) {
        self.0 = self.0.saturating_add(n);
    }
}

impl From<u32> for DroppedCount {
    fn from(n: u32) -> Self {
        Self(n)
    }
}

impl From<DroppedCount> for u32 {
    fn from(n: DroppedCount) -> Self {
        n.0
    }
}

impl ByteSizeOf for DroppedCount {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// Resource attached to a [`super::TraceEvent`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Resource {
    /// `service.name`.
    pub service: Option<String>,
    /// `deployment.environment.name`.
    pub environment: Option<String>,
    /// `host.name`.
    pub host: Option<String>,
    /// Remaining resource attributes.
    pub attributes: Attributes,
    /// OTLP schema URL.
    pub schema_url: Option<String>,
    /// Dropped resource attributes.
    pub dropped_attributes_count: DroppedCount,
}

impl ByteSizeOf for Resource {
    fn allocated_bytes(&self) -> usize {
        allocated_optional_string(self.service.as_deref())
            + allocated_optional_string(self.environment.as_deref())
            + allocated_optional_string(self.host.as_deref())
            + self.attributes.allocated_bytes()
            + allocated_optional_string(self.schema_url.as_deref())
    }
}

/// Instrumentation scope attached to a [`super::TraceEvent`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Scope {
    /// Scope name. `None` is OTLP "instrumentation scope name unknown".
    pub name: Option<String>,
    /// Scope version.
    pub version: Option<String>,
    /// Scope attributes.
    pub attributes: Attributes,
    /// OTLP schema URL.
    pub schema_url: Option<String>,
    /// Dropped scope attributes.
    pub dropped_attributes_count: DroppedCount,
}

impl ByteSizeOf for Scope {
    fn allocated_bytes(&self) -> usize {
        allocated_optional_string(self.name.as_deref())
            + allocated_optional_string(self.version.as_deref())
            + self.attributes.allocated_bytes()
            + allocated_optional_string(self.schema_url.as_deref())
    }
}

/// A span belonging to its enclosing [`super::TraceEvent`]'s trace ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Span {
    /// Span identifier.
    pub span_id: SpanId,
    /// Parent span identifier. `None` for roots and for empty/zero OTLP parent IDs.
    pub parent_span_id: Option<SpanId>,
    /// Raw W3C `tracestate`.
    pub trace_state: TraceState,
    /// OTLP flags bitfield.
    pub flags: TraceFlags,
    /// Span name.
    pub name: String,
    /// Span kind.
    pub kind: SpanKind,
    /// Start timestamp.
    pub start_time: DateTime<Utc>,
    /// Duration with nanosecond precision.
    pub duration: Duration,
    /// Span status.
    pub status: SpanStatus,
    /// Datadog-native span state. Default means none is present.
    pub datadog: DatadogSpanContext,
    /// Per-span attributes.
    pub attributes: Attributes,
    /// Span events.
    pub events: Vec<SpanEvent>,
    /// Span links.
    pub links: Vec<SpanLink>,
    /// Dropped attributes.
    pub dropped_attributes_count: DroppedCount,
    /// Dropped events.
    pub dropped_events_count: DroppedCount,
    /// Dropped links.
    pub dropped_links_count: DroppedCount,
}

impl Span {
    /// Constructs a span with the given ID and name, and otherwise default fields.
    #[must_use]
    pub fn new(span_id: SpanId, name: impl Into<String>) -> Self {
        Self {
            span_id,
            parent_span_id: None,
            trace_state: TraceState::default(),
            flags: TraceFlags::empty(),
            name: name.into(),
            kind: SpanKind::Unspecified,
            start_time: DateTime::<Utc>::UNIX_EPOCH,
            duration: Duration::ZERO,
            status: SpanStatus::Unset,
            datadog: DatadogSpanContext::default(),
            attributes: Attributes::new(),
            events: Vec::new(),
            links: Vec::new(),
            dropped_attributes_count: DroppedCount::ZERO,
            dropped_events_count: DroppedCount::ZERO,
            dropped_links_count: DroppedCount::ZERO,
        }
    }
}

impl ByteSizeOf for Span {
    fn allocated_bytes(&self) -> usize {
        self.trace_state.allocated_bytes()
            + self.name.len()
            + self.status.allocated_bytes()
            + self.datadog.allocated_bytes()
            + self.attributes.allocated_bytes()
            + self.events.allocated_bytes()
            + self.links.allocated_bytes()
    }
}

/// Timed annotation on a span.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpanEvent {
    /// Event name.
    pub name: String,
    /// Timestamp. Unix epoch means "timestamp unknown" per OTLP.
    pub time: DateTime<Utc>,
    /// Event attributes.
    pub attributes: Attributes,
    /// Dropped attributes.
    pub dropped_attributes_count: DroppedCount,
}

impl ByteSizeOf for SpanEvent {
    fn allocated_bytes(&self) -> usize {
        self.name.len() + self.attributes.allocated_bytes()
    }
}

/// Link from a span to another span, possibly in another trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpanLink {
    /// Target trace ID, which may differ from the enclosing event.
    pub trace_id: TraceId,
    /// Target span ID.
    pub span_id: SpanId,
    /// Raw W3C `tracestate`.
    pub trace_state: TraceState,
    /// OTLP flags bitfield.
    pub flags: TraceFlags,
    /// Link attributes.
    pub attributes: Attributes,
    /// Dropped attributes.
    pub dropped_attributes_count: DroppedCount,
}

impl ByteSizeOf for SpanLink {
    fn allocated_bytes(&self) -> usize {
        self.trace_state.allocated_bytes() + self.attributes.allocated_bytes()
    }
}

fn allocated_optional_string(value: Option<&str>) -> usize {
    value.map_or(0, str::len)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::{DateTime, Utc};
    use similar_asserts::assert_eq;
    use vector_common::byte_size_of::ByteSizeOf;

    use super::{DroppedCount, Resource, Scope, Span, SpanId};

    fn span_id(n: u64) -> SpanId {
        SpanId::new(n).expect("non-zero test id")
    }

    #[test]
    fn dropped_counts_saturate() {
        let mut count = DroppedCount::MAX;
        count.increment(1);
        assert_eq!(count, DroppedCount::MAX);

        let mut span = Span::new(span_id(1), "s");
        span.dropped_attributes_count = DroppedCount::MAX;
        span.dropped_attributes_count.increment(1);
        assert_eq!(span.dropped_attributes_count, DroppedCount::MAX);

        span.dropped_events_count = DroppedCount::new(u32::MAX - 1);
        span.dropped_events_count.increment(5);
        assert_eq!(span.dropped_events_count, DroppedCount::MAX);

        span.dropped_links_count.increment(3);
        assert_eq!(span.dropped_links_count, DroppedCount::new(3));

        let mut resource = Resource {
            dropped_attributes_count: DroppedCount::MAX,
            ..Resource::default()
        };
        resource.dropped_attributes_count.increment(10);
        assert_eq!(resource.dropped_attributes_count, DroppedCount::MAX);

        let mut scope = Scope::default();
        scope.dropped_attributes_count.increment(2);
        assert_eq!(scope.dropped_attributes_count, DroppedCount::new(2));
    }

    #[test]
    fn allocated_size_includes_span_name() {
        let short = Span::new(span_id(1), "a").allocated_bytes();
        let long = Span::new(span_id(1), "a-very-long-span-name").allocated_bytes();
        assert!(long > short);
    }

    #[test]
    fn empty_optional_string_slots_preserve_some_empty() {
        let resource = Resource {
            service: Some(String::new()),
            ..Resource::default()
        };
        assert_eq!(resource.service.as_deref(), Some(""));
    }

    #[test]
    fn unix_epoch_start_is_unknown_timestamp() {
        let span = Span::new(span_id(1), "s");
        assert_eq!(span.start_time, DateTime::<Utc>::UNIX_EPOCH);
        assert_eq!(span.duration, Duration::ZERO);
    }
}
