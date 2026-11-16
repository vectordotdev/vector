//! Constructors shared by the typed-trace unit tests.

use std::time::Duration;

use crate::proto::{
    common::v1::{
        AnyValue, InstrumentationScope, KeyValue, KeyValueList, any_value::Value as PbValue,
    },
    resource::v1::Resource as PbResource,
    trace::v1::{
        ResourceSpans, ScopeSpans, Span as PbSpan, Status as PbStatus, span::Link as PbLink,
    },
};
use chrono::DateTime;
use vector_core::event::{
    EventMetadata,
    typed_trace::{
        AttrMap, AttrValue, Attributes, DroppedCount, OtlpSpanFlags, PanicOnIssue, Resource, Span,
        SpanId, SpanKind, SpanLink, SpanStatus, TraceConversionCounts, TraceEvent, TraceId,
        TraceState,
    },
};

pub(crate) fn tid(n: u128) -> TraceId {
    TraceId::new(n).unwrap()
}

pub(crate) fn sid(n: u64) -> SpanId {
    SpanId::new(n).unwrap()
}

impl AnyValue {
    pub(crate) fn from_string(value: impl Into<String>) -> Self {
        Self {
            value: Some(PbValue::StringValue(value.into())),
        }
    }

    pub(crate) fn from_int(value: i64) -> Self {
        Self {
            value: Some(PbValue::IntValue(value)),
        }
    }

    pub(crate) fn from_bool(value: bool) -> Self {
        Self {
            value: Some(PbValue::BoolValue(value)),
        }
    }

    pub(crate) fn from_bytes(value: impl Into<Vec<u8>>) -> Self {
        Self {
            value: Some(PbValue::BytesValue(value.into())),
        }
    }

    pub(crate) fn from_double(value: f64) -> Self {
        Self {
            value: Some(PbValue::DoubleValue(value)),
        }
    }

    pub(crate) fn from_kvlist(values: Vec<KeyValue>) -> Self {
        Self {
            value: Some(PbValue::KvlistValue(KeyValueList { values })),
        }
    }
}

impl KeyValue {
    pub(crate) fn from_parts(key: impl Into<String>, value: Option<AnyValue>) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }

    pub(crate) fn from_string(key: &str, value: &str) -> Self {
        Self::from_parts(key, Some(AnyValue::from_string(value)))
    }

    pub(crate) fn from_int(key: &str, value: i64) -> Self {
        Self::from_parts(key, Some(AnyValue::from_int(value)))
    }
}

impl PbSpan {
    pub(crate) fn from_parts(trace: u128, span: u64, name: &str) -> Self {
        Self {
            trace_id: tid(trace).to_bytes().to_vec(),
            span_id: sid(span).to_bytes().to_vec(),
            name: name.to_owned(),
            kind: SpanKind::Internal.to_i32(),
            start_time_unix_nano: 1_000,
            end_time_unix_nano: 2_000,
            status: Some(PbStatus {
                code: SpanStatus::UNSET,
                message: String::new(),
            }),
            ..Self::default()
        }
    }
}

impl PbLink {
    pub(crate) fn from_parts(trace: u128, span: u64) -> Self {
        Self {
            trace_id: tid(trace).to_bytes().to_vec(),
            span_id: sid(span).to_bytes().to_vec(),
            ..Self::default()
        }
    }
}

impl ScopeSpans {
    pub(crate) fn from_spans(spans: Vec<PbSpan>) -> Self {
        Self {
            scope: Some(InstrumentationScope::default()),
            spans,
            schema_url: String::new(),
        }
    }
}

impl ResourceSpans {
    pub(crate) fn from_spans(spans: Vec<PbSpan>) -> Self {
        Self::from_parts(spans, Vec::new())
    }

    pub(crate) fn from_parts(spans: Vec<PbSpan>, attributes: Vec<KeyValue>) -> Self {
        Self {
            resource: Some(PbResource {
                attributes,
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans::from_spans(spans)],
            schema_url: String::new(),
        }
    }

    pub(crate) fn decode(self) -> (Vec<TraceEvent>, TraceConversionCounts) {
        let mut counts = TraceConversionCounts::default();
        let events = self.into_typed_events(&EventMetadata::default(), &mut counts);
        (events, counts)
    }

    pub(crate) fn convert(self) -> Vec<TraceEvent> {
        self.into_typed_events(&EventMetadata::default(), &mut PanicOnIssue)
    }
}

pub(crate) fn attr_map<const N: usize>(entries: [(&str, AttrValue); N]) -> AttrMap {
    entries.into_iter().map(|(k, v)| (k.into(), v)).collect()
}

pub(crate) fn attrs<const N: usize>(entries: [(&str, AttrValue); N]) -> Attributes {
    Attributes::from(attr_map(entries))
}

/// Typed counterpart of [`PbSpan::from_parts`].
pub(crate) fn typed_span(span: u64, name: &str) -> Span {
    Span {
        kind: SpanKind::Internal,
        start_time: DateTime::from_timestamp_nanos(1_000),
        duration: Duration::from_micros(1),
        ..Span::new(sid(span), name)
    }
}

/// Typed counterpart of [`PbLink::from_parts`].
pub(crate) fn typed_link(trace: u128, span: u64) -> SpanLink {
    SpanLink {
        trace_id: tid(trace),
        span_id: sid(span),
        trace_state: TraceState::default(),
        flags: OtlpSpanFlags::none(),
        attributes: Attributes::new(),
        dropped_attributes_count: DroppedCount::ZERO,
    }
}

pub(crate) fn typed_event_with_metadata(
    trace: u128,
    spans: Vec<Span>,
    metadata: &EventMetadata,
) -> TraceEvent {
    let mut event = TraceEvent::new_with_metadata(tid(trace), metadata.clone());
    *event.spans_mut() = spans;
    event
}

pub(crate) fn typed_event(trace: u128, spans: Vec<Span>) -> TraceEvent {
    typed_event_with_metadata(trace, spans, &EventMetadata::default())
}

/// The outcome of converting one grouping whose spans are all rejected.
pub(crate) fn rejected(rejected_spans: usize) -> (Vec<TraceEvent>, TraceConversionCounts) {
    (
        Vec::new(),
        TraceConversionCounts {
            rejected_spans,
            discarded_groupings: 1,
            ..TraceConversionCounts::default()
        },
    )
}

/// The event decoded from `ResourceSpans::from_parts` with one `PbSpan::from_parts(1, 1, "s")`.
pub(crate) fn resource_event(resource: Resource) -> TraceEvent {
    let mut event = typed_event(1, vec![typed_span(1, "s")]);
    *event.resource_mut() = resource;
    event
}
