//! Helpers for typed Datadog egress tests.

use crate::event::typed_trace::{
    Span, SpanId, TraceConversionIssue, TraceConversionReporter, TraceEvent, TraceId,
};

pub(super) fn trace_id(id: u128) -> TraceId {
    TraceId::new(id).expect("non-zero trace id")
}

pub(super) fn span_id(id: u64) -> SpanId {
    SpanId::new(id).expect("non-zero span id")
}

pub(super) fn span(id: u64, name: &str) -> Span {
    Span::new(span_id(id), name)
}

pub(super) fn event(trace: u128, service: Option<&str>, spans: Vec<Span>) -> TraceEvent {
    let mut event = TraceEvent::new(trace_id(trace));
    event.resource_mut().service = service.map(str::to_owned);
    *event.spans_mut() = spans;
    event
}

/// Issues recorded while encoding. Unexpected variants fail the calling assertion.
#[derive(Debug, Default)]
pub(super) struct Issues {
    /// [`TraceConversionIssue::AttributeCollision`] keys, in report order.
    pub collisions: Vec<String>,
    /// [`TraceConversionIssue::MalformedAttribute`] count.
    pub malformed_attributes: usize,
    /// [`TraceConversionIssue::EmptyEvent`] count.
    pub empty_events: usize,
    /// [`TraceConversionIssue::ClampedTimestamp`] count.
    pub clamped_timestamps: usize,
    /// Anything else, rendered for the assertion.
    pub unexpected: Vec<String>,
}

impl TraceConversionReporter for Issues {
    fn report(&mut self, issue: TraceConversionIssue<'_>) {
        match issue {
            TraceConversionIssue::AttributeCollision { key } => {
                self.collisions.push(key.to_owned());
            }
            TraceConversionIssue::MalformedAttribute => self.malformed_attributes += 1,
            TraceConversionIssue::EmptyEvent => self.empty_events += 1,
            TraceConversionIssue::ClampedTimestamp => self.clamped_timestamps += 1,
            other => self.unexpected.push(format!("{other:?}")),
        }
    }
}
