//! Issue reporting for wire-format conversion.

use super::InvalidIdError;

/// Identifier field holding an invalid value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdField {
    /// Trace ID.
    TraceId,
    /// Span ID.
    SpanId,
    /// Parent span ID.
    ParentSpanId,
}

/// A drop or normalization found while converting between a wire format and typed traces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceConversionIssue<'a> {
    /// A span was rejected for an invalid identifier.
    RejectedSpan {
        /// Field holding the invalid identifier.
        field: IdField,
        /// Why the identifier is invalid.
        error: InvalidIdError,
    },
    /// A link was dropped for an invalid identifier. The enclosing span's
    /// `dropped_links_count` also counts it.
    DroppedLink {
        /// Field holding the invalid identifier.
        field: IdField,
        /// Why the identifier is invalid.
        error: InvalidIdError,
    },
    /// A malformed resource or scope grouping was dropped, including any spans it held.
    MalformedGrouping,
    /// A resource or scope grouping produced no event because it held no accepted spans, so
    /// its resource and scope state was not relayed.
    DiscardedGrouping,
    /// An event with no spans was encoded as an empty grouping, which cannot carry its trace
    /// ID or Datadog chunk context.
    EmptyEvent,
    /// A present Datadog chunk context held only default values, so the destination cannot
    /// distinguish it from an absent one.
    UnrepresentableChunkContext,
    /// A legacy-layout field held the wrong type or an unparseable value, so its default was
    /// used.
    MalformedField {
        /// Field name.
        field: &'a str,
    },
    /// A legacy-layout field was discarded because the typed model has no place for it: an
    /// unknown key, or an alias shadowed by its other spelling.
    UnmappedField {
        /// Discarded key.
        key: &'a str,
    },
    /// A malformed span event was dropped. The enclosing span's `dropped_events_count` also
    /// counts it.
    MalformedEvent,
    /// A malformed attribute entry was dropped. The enclosing item's
    /// `dropped_attributes_count` also counts it.
    MalformedAttribute,
    /// An attribute value was replaced by a later duplicate of its key.
    DuplicateAttribute {
        /// Duplicated key.
        key: &'a str,
    },
    /// An attribute was discarded because another key or a typed slot supplies its value.
    AttributeCollision {
        /// Discarded key.
        key: &'a str,
    },
    /// A reserved Datadog bridge attribute was malformed, at the wrong scope, had an unknown
    /// member, or was not synthesized from its typed slot.
    InvalidReservedAttribute {
        /// Reserved key.
        key: &'a str,
    },
    /// A Datadog chunk bridge value conflicted with an earlier span's in the same partition.
    ConflictingChunkContext {
        /// Conflicting key.
        key: &'a str,
    },
    /// A status message was discarded from an `Unset` or `Ok` status.
    NonconformingStatus,
    /// A span ended before it started, so its duration was clamped to zero.
    ReversedSpanTimestamps,
    /// A timestamp was clamped to the destination field's domain.
    ClampedTimestamp,
}

impl TraceConversionIssue<'_> {
    /// Bounded error code for component-error metrics.
    ///
    /// The set is fixed so it can be a metric tag. Attribute keys and field names stay
    /// out of it.
    #[must_use]
    pub const fn error_code(self) -> &'static str {
        match self {
            Self::RejectedSpan {
                field: IdField::TraceId,
                ..
            }
            | Self::DroppedLink {
                field: IdField::TraceId,
                ..
            } => "invalid_trace_id",
            Self::RejectedSpan {
                field: IdField::SpanId | IdField::ParentSpanId,
                ..
            }
            | Self::DroppedLink {
                field: IdField::SpanId | IdField::ParentSpanId,
                ..
            } => "invalid_span_id",
            Self::MalformedGrouping | Self::DiscardedGrouping => "discarded_span",
            Self::EmptyEvent => "discarded_empty_chunk",
            Self::UnrepresentableChunkContext => "unrepresentable_chunk_context",
            Self::MalformedField { .. } | Self::MalformedAttribute => "invalid_attribute_value",
            Self::UnmappedField { .. } => "legacy_conversion_error",
            Self::MalformedEvent => "discarded_event",
            Self::DuplicateAttribute { .. } => "duplicate_attribute",
            Self::AttributeCollision { .. } => "attribute_collision",
            Self::InvalidReservedAttribute { .. } => "invalid_reserved_attribute",
            Self::ConflictingChunkContext { .. } => "conflicting_chunk_context",
            Self::NonconformingStatus => "nonconforming_status",
            Self::ReversedSpanTimestamps => "reversed_span_timestamps",
            Self::ClampedTimestamp => "timestamp_clamped",
        }
    }

    /// Operator-facing description of this issue.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::RejectedSpan { .. } => {
                "Trace span was dropped because an identifier was invalid."
            }
            Self::DroppedLink { .. } => "Trace link was dropped because an identifier was invalid.",
            Self::MalformedGrouping => "Trace grouping was dropped because it was malformed.",
            Self::DiscardedGrouping => {
                "Trace grouping was dropped because it had no accepted spans."
            }
            Self::EmptyEvent => "Empty trace event was encoded without its trace ID.",
            Self::UnrepresentableChunkContext => {
                "Datadog chunk context could not be distinguished from an absent one."
            }
            Self::MalformedField { .. } => {
                "Trace field had the wrong type, so its default was used."
            }
            Self::UnmappedField { .. } => {
                "Trace field was discarded because the typed model has no place for it."
            }
            Self::MalformedEvent => "Trace span event was dropped because it was malformed.",
            Self::MalformedAttribute => "Trace attribute was dropped because it was malformed.",
            Self::DuplicateAttribute { .. } => {
                "Trace attribute was replaced by a later duplicate of its key."
            }
            Self::AttributeCollision { .. } => {
                "Trace attribute was discarded because another field supplies its value."
            }
            Self::InvalidReservedAttribute { .. } => {
                "Reserved Datadog bridge attribute was malformed."
            }
            Self::ConflictingChunkContext { .. } => {
                "Datadog chunk context conflicted with an earlier span."
            }
            Self::NonconformingStatus => {
                "Trace status message was discarded from an unset or ok status."
            }
            Self::ReversedSpanTimestamps => {
                "Trace span ended before it started, so its duration was clamped to zero."
            }
            Self::ClampedTimestamp => {
                "Trace timestamp was clamped to the destination field's domain."
            }
        }
    }
}

/// Receives conversion issues so the caller can report them.
pub trait TraceConversionReporter {
    /// Records one issue.
    fn report(&mut self, issue: TraceConversionIssue<'_>);
}

/// Reporter for conversions that must not report any issue; panics on the first one.
#[cfg(any(test, feature = "test"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct PanicOnIssue;

#[cfg(any(test, feature = "test"))]
impl TraceConversionReporter for PanicOnIssue {
    fn report(&mut self, issue: TraceConversionIssue<'_>) {
        panic!("unexpected trace conversion issue: {issue:?}");
    }
}

/// Reported conversion issues, counted by category.
#[cfg(any(test, feature = "test"))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TraceConversionCounts {
    /// [`TraceConversionIssue::RejectedSpan`] reports.
    pub rejected_spans: usize,
    /// [`TraceConversionIssue::DroppedLink`] reports.
    pub dropped_links: usize,
    /// [`TraceConversionIssue::MalformedGrouping`] reports.
    pub malformed_groupings: usize,
    /// [`TraceConversionIssue::DiscardedGrouping`] reports.
    pub discarded_groupings: usize,
    /// [`TraceConversionIssue::EmptyEvent`] reports.
    pub empty_events: usize,
    /// [`TraceConversionIssue::UnrepresentableChunkContext`] reports.
    pub unrepresentable_chunk_contexts: usize,
    /// [`TraceConversionIssue::MalformedField`] reports.
    pub malformed_fields: usize,
    /// [`TraceConversionIssue::UnmappedField`] reports.
    pub unmapped_fields: usize,
    /// [`TraceConversionIssue::MalformedEvent`] reports.
    pub malformed_events: usize,
    /// [`TraceConversionIssue::MalformedAttribute`] reports.
    pub malformed_attributes: usize,
    /// [`TraceConversionIssue::DuplicateAttribute`] reports.
    pub duplicate_attributes: usize,
    /// [`TraceConversionIssue::AttributeCollision`] reports.
    pub attribute_collisions: usize,
    /// [`TraceConversionIssue::InvalidReservedAttribute`] reports.
    pub invalid_reserved_attributes: usize,
    /// [`TraceConversionIssue::ConflictingChunkContext`] reports.
    pub conflicting_chunk_contexts: usize,
    /// [`TraceConversionIssue::NonconformingStatus`] reports.
    pub nonconforming_statuses: usize,
    /// [`TraceConversionIssue::ReversedSpanTimestamps`] reports.
    pub reversed_span_timestamps: usize,
    /// [`TraceConversionIssue::ClampedTimestamp`] reports.
    pub clamped_timestamps: usize,
}

#[cfg(any(test, feature = "test"))]
impl TraceConversionReporter for TraceConversionCounts {
    fn report(&mut self, issue: TraceConversionIssue<'_>) {
        match issue {
            TraceConversionIssue::RejectedSpan { .. } => self.rejected_spans += 1,
            TraceConversionIssue::DroppedLink { .. } => self.dropped_links += 1,
            TraceConversionIssue::MalformedGrouping => self.malformed_groupings += 1,
            TraceConversionIssue::DiscardedGrouping => self.discarded_groupings += 1,
            TraceConversionIssue::EmptyEvent => self.empty_events += 1,
            TraceConversionIssue::UnrepresentableChunkContext => {
                self.unrepresentable_chunk_contexts += 1;
            }
            TraceConversionIssue::MalformedField { .. } => self.malformed_fields += 1,
            TraceConversionIssue::UnmappedField { .. } => self.unmapped_fields += 1,
            TraceConversionIssue::MalformedEvent => self.malformed_events += 1,
            TraceConversionIssue::MalformedAttribute => self.malformed_attributes += 1,
            TraceConversionIssue::DuplicateAttribute { .. } => self.duplicate_attributes += 1,
            TraceConversionIssue::AttributeCollision { .. } => self.attribute_collisions += 1,
            TraceConversionIssue::InvalidReservedAttribute { .. } => {
                self.invalid_reserved_attributes += 1;
            }
            TraceConversionIssue::ConflictingChunkContext { .. } => {
                self.conflicting_chunk_contexts += 1;
            }
            TraceConversionIssue::NonconformingStatus => self.nonconforming_statuses += 1,
            TraceConversionIssue::ReversedSpanTimestamps => self.reversed_span_timestamps += 1,
            TraceConversionIssue::ClampedTimestamp => self.clamped_timestamps += 1,
        }
    }
}
