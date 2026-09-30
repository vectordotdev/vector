//! Tests for [`super::TraceEvent`].
//!
//! Type-local tests live next to their types. This file stays with `mod.rs` because it
//! exercises event-level behavior that composes identifiers, spans, metadata, and finalizers.

use similar_asserts::assert_eq;
use vector_buffers::EventCount;
use vector_common::{
    byte_size_of::ByteSizeOf,
    finalization::{BatchNotifier, BatchStatus, EventFinalizer, EventStatus, Finalizable},
};

use super::{Span, SpanId, TraceEvent, TraceId};

fn trace_id(n: u128) -> TraceId {
    TraceId::new(n).expect("non-zero test id")
}

fn span_id(n: u64) -> SpanId {
    SpanId::new(n).expect("non-zero test id")
}

#[test]
fn empty_event_retains_trace_id() {
    let id = trace_id(0xDEAD_BEEF);
    let mut event = TraceEvent::new(id);
    assert_eq!(event.trace_id(), id);
    assert!(event.spans().is_empty());
    assert_eq!(event.event_count(), 1);

    event.spans_mut().push(Span::new(span_id(1), "root"));
    event.spans_mut().clear();
    assert_eq!(event.trace_id(), id);
    assert!(event.spans().is_empty());
}

#[test]
fn span_insertion_does_not_rewrite_event_id() {
    let mut event = TraceEvent::new(trace_id(1));
    event.set_trace_id(trace_id(2));
    event.spans_mut().push(Span::new(span_id(9), "child"));
    assert_eq!(event.trace_id(), trace_id(2));
    assert_eq!(event.spans()[0].span_id, span_id(9));
}

#[test]
fn metadata_clone_shares_until_mutation() {
    let mut event = TraceEvent::new(trace_id(1));
    event.metadata_mut().set_source_type("datadog_agent");
    let clone = event.clone();
    assert_eq!(
        event.metadata().source_type(),
        clone.metadata().source_type()
    );

    event.metadata_mut().set_source_type("opentelemetry");
    assert_eq!(event.metadata().source_type(), Some("opentelemetry"));
    assert_eq!(clone.metadata().source_type(), Some("datadog_agent"));
}

#[test]
fn cloned_events_share_finalizers() {
    let (batch, mut rx) = BatchNotifier::new_with_receiver();
    let mut original = TraceEvent::new(trace_id(1));
    original.add_finalizer(EventFinalizer::new(batch));

    let mut fan_out = original.clone();
    assert_eq!(fan_out.event_count(), 1);
    assert_eq!(original.metadata().finalizers().len(), 1);
    assert_eq!(
        original.metadata().finalizers(),
        fan_out.metadata().finalizers()
    );

    let first = original.take_finalizers();
    first.update_status(EventStatus::Delivered);
    drop(first);
    assert!(original.metadata().finalizers().is_empty());
    assert_eq!(fan_out.metadata().finalizers().len(), 1);

    let second = fan_out.take_finalizers();
    second.update_status(EventStatus::Delivered);
    drop(second);
    assert_eq!(rx.try_recv(), Ok(BatchStatus::Delivered));
}

#[test]
fn event_count_is_one_regardless_of_span_count() {
    let mut event = TraceEvent::new(trace_id(1));
    assert_eq!(event.event_count(), 1);
    for i in 1..=8 {
        event.spans_mut().push(Span::new(span_id(i), "s"));
    }
    assert_eq!(event.event_count(), 1);
}

#[test]
fn allocated_size_includes_span_name() {
    let mut event = TraceEvent::new(trace_id(1));
    let before = event.allocated_bytes();
    event
        .spans_mut()
        .push(Span::new(span_id(1), "a-very-long-span-name"));
    assert!(event.allocated_bytes() > before);
}
