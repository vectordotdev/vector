//! Shared conversion of Stage 1 legacy [`vector_core::event::TraceEvent`]s into typed traces.
//!
//! Datadog parsing lives here so the `datadog_traces` sink can convert without
//! enabling the `datadog_agent` source. The sink dispatcher uses a present Stage 1
//! layout hint when it is recognized; otherwise it requires exactly one historical
//! detector to match. The retaining wrapper returns the original legacy event on failure.
//!
//! Pulled into Vector only when the `typed-trace` cargo feature is enabled.

pub mod datadog;
#[cfg(test)]
mod tests;

use vector_core::event::{
    TraceEvent as LegacyTraceEvent, TraceLayout,
    typed_trace::{TraceConversionReporter, TraceEvent},
};

/// Failure selecting or applying a legacy-to-typed converter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectError {
    /// `%vector.trace_layout` is present but is not a recognized producer hint.
    UnknownHint,
    /// No hint, and both the OTLP and Datadog historical detectors matched.
    AmbiguousLayout,
    /// The selected converter rejected the record, or no detector matched.
    UnrecognizedLayout,
}

impl std::fmt::Display for SelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownHint => f.write_str("unrecognized trace layout hint"),
            Self::AmbiguousLayout => {
                f.write_str("legacy trace event matched both OTLP and Datadog detectors")
            }
            Self::UnrecognizedLayout => {
                f.write_str("legacy trace event is not a recognized OTLP or Datadog layout")
            }
        }
    }
}

impl std::error::Error for SelectError {}

/// Converts a hinted or uniquely detected legacy trace into typed events.
///
/// Zero typed outputs is success (for example an empty Datadog chunk).
///
/// Conversion issues found while decoding an OTLP-shaped record are sent to `reporter`.
/// Datadog conversion currently does not emit conversion issues.
///
/// # Errors
///
/// Returns [`SelectError`] when the hint is unknown, both historical detectors
/// match, or the selected converter rejects the record.
pub fn convert_legacy(
    legacy: &LegacyTraceEvent,
    reporter: &mut impl TraceConversionReporter,
) -> Result<Vec<TraceEvent>, SelectError> {
    match legacy.metadata().trace_layout() {
        Some(TraceLayout::Datadog) => {
            datadog::legacy_to_typed(legacy).map_err(|_| SelectError::UnrecognizedLayout)
        }
        Some(TraceLayout::OtlpResourceSpans | TraceLayout::OtelFlattened) => {
            convert_otlp(legacy, reporter)
        }
        Some(_) => Err(SelectError::UnknownHint),
        None => detect_and_convert(legacy, reporter),
    }
}

/// Converts `legacy`, or returns it unchanged when conversion fails.
///
/// An empty typed output is success and does not return the original event.
/// Conversion issues found while decoding an OTLP-shaped record are sent to `reporter`.
/// Datadog conversion currently does not emit conversion issues.
///
/// # Errors
///
/// Returns `legacy` in the `Err` variant when layout selection or conversion fails.
pub fn convert_or_retain(
    legacy: LegacyTraceEvent,
    reporter: &mut impl TraceConversionReporter,
) -> Result<Vec<TraceEvent>, LegacyTraceEvent> {
    match convert_legacy(&legacy, reporter) {
        Ok(events) => Ok(events),
        Err(_) => Err(legacy),
    }
}

fn detect_and_convert(
    legacy: &LegacyTraceEvent,
    reporter: &mut impl TraceConversionReporter,
) -> Result<Vec<TraceEvent>, SelectError> {
    match (
        opentelemetry_proto::typed_trace::is_historical_otlp_layout(legacy),
        datadog::is_historical_datadog_layout(legacy),
    ) {
        (true, false) => convert_otlp(legacy, reporter),
        (false, true) => {
            datadog::legacy_to_typed(legacy).map_err(|_| SelectError::UnrecognizedLayout)
        }
        (true, true) => Err(SelectError::AmbiguousLayout),
        (false, false) => Err(SelectError::UnrecognizedLayout),
    }
}

fn convert_otlp(
    legacy: &LegacyTraceEvent,
    reporter: &mut impl TraceConversionReporter,
) -> Result<Vec<TraceEvent>, SelectError> {
    opentelemetry_proto::typed_trace::legacy_to_typed(legacy.clone(), reporter)
        .map_err(|_| SelectError::UnrecognizedLayout)
}
