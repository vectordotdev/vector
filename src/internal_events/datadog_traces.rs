use vector_lib::internal_event::{
    ComponentEventsDropped, CounterName, InternalEvent, UNINTENTIONAL, emit, error_stage,
    error_type,
};
use vector_lib::{NamedInternalEvent, counter};

#[cfg(feature = "typed-trace")]
use crate::event::typed_trace::TraceConversionIssue;

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct DatadogTracesEncodingError {
    pub error_message: &'static str,
    pub error_reason: String,
    pub dropped_events: usize,
}

impl InternalEvent for DatadogTracesEncodingError {
    fn emit(self) {
        let reason = "Failed to encode Datadog traces.";
        error!(
            message = reason,
            error = %self.error_message,
            error_reason = %self.error_reason,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        if self.dropped_events > 0 {
            emit(ComponentEventsDropped::<UNINTENTIONAL> {
                count: self.dropped_events,
                reason,
            });
        }
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct DatadogTracesAPMStatsError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for DatadogTracesAPMStatsError<E> {
    fn emit(self) {
        error!(
            message = "Failed sending APM stats payload.",
            error = %self.error,
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);

        // No dropped events because APM stats payloads are not considered events.
    }
}

/// A typed-trace value was normalized or dropped while encoding continued.
///
/// The error code is a bounded label derived from `issue`. The key or identifier
/// is logged only; it is not a metric tag.
#[cfg(feature = "typed-trace")]
#[derive(Debug, NamedInternalEvent)]
pub(crate) struct DatadogTracesConversionError<'a> {
    /// Conversion or grouping issue being reported.
    pub issue: TraceConversionIssue<'a>,
}

#[cfg(feature = "typed-trace")]
impl DatadogTracesConversionError<'_> {
    /// Bounded error code, stable log message, and the key or identifier to log.
    fn details(&self) -> (&'static str, &'static str, String) {
        match self.issue {
            TraceConversionIssue::RejectedSpan { field, error } => (
                field.error_code(),
                "A span was dropped because its identifier is invalid.",
                format!("{} ({error})", field.as_str()),
            ),
            TraceConversionIssue::DroppedLink { field, error } => (
                field.error_code(),
                "A span link was dropped because its identifier is invalid.",
                format!("{} ({error})", field.as_str()),
            ),
            TraceConversionIssue::MalformedGrouping => (
                "discarded_span",
                "A malformed grouping and its spans were dropped.",
                String::new(),
            ),
            TraceConversionIssue::DiscardedGrouping => (
                "discarded_empty_chunk",
                "A grouping with no accepted spans was discarded.",
                String::new(),
            ),
            TraceConversionIssue::EmptyEvent => (
                "discarded_empty_chunk",
                "An event with no spans was encoded as an empty chunk.",
                String::new(),
            ),
            TraceConversionIssue::UnrepresentableChunkContext => (
                "invalid_reserved_envelope",
                "A Datadog chunk context could not be distinguished from an absent one.",
                String::new(),
            ),
            TraceConversionIssue::MalformedField { field } => (
                "legacy_layout_error",
                "A legacy field had the wrong type, so its default was used.",
                field.to_owned(),
            ),
            TraceConversionIssue::UnmappedField { key } => (
                "legacy_conversion_error",
                "A legacy field was discarded because the typed model has no place for it.",
                key.to_owned(),
            ),
            TraceConversionIssue::MalformedEvent => (
                "discarded_event",
                "A malformed span event was dropped.",
                String::new(),
            ),
            TraceConversionIssue::MalformedAttribute => (
                "invalid_attribute_value",
                "A malformed attribute was dropped.",
                String::new(),
            ),
            TraceConversionIssue::DuplicateAttribute { key } => (
                "duplicate_attribute",
                "A duplicate attribute replaced an earlier value.",
                key.to_owned(),
            ),
            TraceConversionIssue::AttributeCollision { key } => (
                "attribute_collision",
                "An attribute was discarded because another field supplies its value.",
                key.to_owned(),
            ),
            TraceConversionIssue::InvalidReservedAttribute { key } => (
                "invalid_reserved_attribute",
                "A reserved Datadog attribute was discarded.",
                key.to_owned(),
            ),
            TraceConversionIssue::ConflictingChunkContext { key } => (
                "conflicting_chunk_context",
                "A Datadog chunk value conflicted with an earlier span.",
                key.to_owned(),
            ),
            TraceConversionIssue::NonconformingStatus => (
                "nonconforming_status",
                "A status message was discarded from an unset or ok status.",
                String::new(),
            ),
            TraceConversionIssue::ReversedSpanTimestamps => (
                "reversed_span_timestamps",
                "A span ended before it started, so its duration was clamped to zero.",
                String::new(),
            ),
            TraceConversionIssue::ClampedTimestamp => (
                "timestamp_clamped",
                "A timestamp or duration was clamped to the Datadog field domain.",
                String::new(),
            ),
        }
    }
}

#[cfg(feature = "typed-trace")]
impl InternalEvent for DatadogTracesConversionError<'_> {
    fn emit(self) {
        let (error_code, message, field) = self.details();
        error!(
            message,
            error_code,
            error_type = error_type::CONVERSION_FAILED,
            stage = error_stage::PROCESSING,
            field = %field,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => error_code,
            "error_type" => error_type::CONVERSION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[cfg(all(test, feature = "typed-trace"))]
mod tests {
    use serial_test::serial;
    use vector_lib::{event::MetricValue, internal_event::emit, metrics::Controller};

    use super::DatadogTracesConversionError;
    use crate::event::typed_trace::{IdField, InvalidIdError, TraceConversionIssue};

    #[test]
    fn every_issue_has_a_bounded_error_code() {
        let cases = [
            (
                TraceConversionIssue::RejectedSpan {
                    field: IdField::TraceId,
                    error: InvalidIdError::Zero,
                },
                "invalid_trace_id",
            ),
            (
                TraceConversionIssue::RejectedSpan {
                    field: IdField::SpanId,
                    error: InvalidIdError::Zero,
                },
                "invalid_span_id",
            ),
            (
                TraceConversionIssue::RejectedSpan {
                    field: IdField::ParentSpanId,
                    error: InvalidIdError::Zero,
                },
                "invalid_span_id",
            ),
            (
                TraceConversionIssue::DroppedLink {
                    field: IdField::TraceId,
                    error: InvalidIdError::Zero,
                },
                "invalid_trace_id",
            ),
            (
                TraceConversionIssue::DroppedLink {
                    field: IdField::SpanId,
                    error: InvalidIdError::Zero,
                },
                "invalid_span_id",
            ),
            (TraceConversionIssue::MalformedGrouping, "discarded_span"),
            (
                TraceConversionIssue::DiscardedGrouping,
                "discarded_empty_chunk",
            ),
            (TraceConversionIssue::EmptyEvent, "discarded_empty_chunk"),
            (
                TraceConversionIssue::UnrepresentableChunkContext,
                "invalid_reserved_envelope",
            ),
            (
                TraceConversionIssue::MalformedField { field: "schemaUrl" },
                "legacy_layout_error",
            ),
            (
                TraceConversionIssue::UnmappedField { key: "extra" },
                "legacy_conversion_error",
            ),
            (TraceConversionIssue::MalformedEvent, "discarded_event"),
            (
                TraceConversionIssue::MalformedAttribute,
                "invalid_attribute_value",
            ),
            (
                TraceConversionIssue::DuplicateAttribute { key: "http.method" },
                "duplicate_attribute",
            ),
            (
                TraceConversionIssue::AttributeCollision { key: "_dd.p.tid" },
                "attribute_collision",
            ),
            (
                TraceConversionIssue::InvalidReservedAttribute { key: "_dd" },
                "invalid_reserved_attribute",
            ),
            (
                TraceConversionIssue::ConflictingChunkContext { key: "env" },
                "conflicting_chunk_context",
            ),
            (
                TraceConversionIssue::NonconformingStatus,
                "nonconforming_status",
            ),
            (
                TraceConversionIssue::ReversedSpanTimestamps,
                "reversed_span_timestamps",
            ),
            (TraceConversionIssue::ClampedTimestamp, "timestamp_clamped"),
        ];

        for (issue, error_code) in cases {
            assert_eq!(
                DatadogTracesConversionError { issue }.details().0,
                error_code
            );
        }
    }

    fn counter_value(tags: &[(&str, &str)]) -> f64 {
        Controller::get()
            .expect("metrics controller initialized")
            .capture_metrics()
            .into_iter()
            .find(|metric| {
                metric.name() == "component_errors_total"
                    && tags.iter().all(|(key, value)| {
                        metric
                            .tags()
                            .is_some_and(|tags| tags.get(key) == Some(*value))
                    })
            })
            .map_or(0.0, |metric| match metric.value() {
                MetricValue::Counter { value } => *value,
                other => panic!("expected counter, got {other:?}"),
            })
    }

    #[test]
    #[serial]
    fn conversion_error_increments_component_errors_without_a_field_tag() {
        crate::test_util::trace_init();
        let tags = [
            ("error_code", "attribute_collision"),
            ("error_type", "conversion_failed"),
            ("stage", "processing"),
        ];
        let before = counter_value(&tags);
        let field_tagged = counter_value(&[
            ("error_code", "attribute_collision"),
            ("field", "_dd.p.tid"),
        ]);

        emit(DatadogTracesConversionError {
            issue: TraceConversionIssue::AttributeCollision { key: "_dd.p.tid" },
        });

        assert_eq!(counter_value(&tags) - before, 1.0);
        assert_eq!(
            counter_value(&[
                ("error_code", "attribute_collision"),
                ("field", "_dd.p.tid"),
            ]),
            field_tagged,
        );
    }
}
