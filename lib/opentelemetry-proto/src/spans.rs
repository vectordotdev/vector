use super::common::{kv_list_into_value, to_hex};
use super::proto::{
    resource::v1::Resource,
    trace::v1::{
        span::{Event as SpanEvent, Link},
        ResourceSpans, Span, Status as SpanStatus,
    },
};
use chrono::{DateTime, TimeZone, Utc};
use std::collections::BTreeMap;
use vector_core::event::{Event, TraceEvent};
use vrl::{
    event_path,
    value::{KeyString, Value},
};

pub const TRACE_ID_KEY: &str = "trace_id";
pub const SPAN_ID_KEY: &str = "span_id";
pub const DROPPED_ATTRIBUTES_COUNT_KEY: &str = "dropped_attributes_count";
pub const RESOURCE_KEY: &str = "resources";
pub const ATTRIBUTES_KEY: &str = "attributes";

impl ResourceSpans {
    pub fn into_event_iter(self) -> impl Iterator<Item = Event> {
        let resource = self.resource;
        let now = Utc::now();

        self.scope_spans
            .into_iter()
            .flat_map(|instrumentation_library_spans| instrumentation_library_spans.spans)
            .map(move |span| {
                ResourceSpan {
                    resource: resource.clone(),
                    span,
                }
                .into_event(now)
            })
    }
}

struct ResourceSpan {
    resource: Option<Resource>,
    span: Span,
}

// Unlike log events(log body + metadata), trace spans are just metadata, so we don't handle log_namespace here,
// insert all attributes into log root, just like what datadog_agent/traces does.
impl ResourceSpan {
    fn into_event(self, now: DateTime<Utc>) -> Event {
        let mut trace = TraceEvent::default();
        let span = self.span;
        trace.insert(
            event_path!(TRACE_ID_KEY),
            Value::from(to_hex(&span.trace_id)),
        );
        trace.insert(event_path!(SPAN_ID_KEY), Value::from(to_hex(&span.span_id)));
        trace.insert(event_path!("trace_state"), span.trace_state);
        trace.insert(
            event_path!("parent_span_id"),
            Value::from(to_hex(&span.parent_span_id)),
        );
        trace.insert(event_path!("name"), span.name);
        trace.insert(event_path!("kind"), span.kind);
        trace.insert(
            event_path!("start_time_unix_nano"),
            Value::from(Utc.timestamp_nanos(span.start_time_unix_nano as i64)),
        );
        trace.insert(
            event_path!("end_time_unix_nano"),
            Value::from(Utc.timestamp_nanos(span.end_time_unix_nano as i64)),
        );
        if !span.attributes.is_empty() {
            trace.insert(
                event_path!(ATTRIBUTES_KEY),
                kv_list_into_value(span.attributes),
            );
        }
        trace.insert(
            event_path!(DROPPED_ATTRIBUTES_COUNT_KEY),
            Value::from(span.dropped_attributes_count),
        );
        if !span.events.is_empty() {
            trace.insert(
                event_path!("events"),
                Value::Array(span.events.into_iter().map(Into::into).collect()),
            );
        }
        trace.insert(
            event_path!("dropped_events_count"),
            Value::from(span.dropped_events_count),
        );
        if !span.links.is_empty() {
            trace.insert(
                event_path!("links"),
                Value::Array(span.links.into_iter().map(Into::into).collect()),
            );
        }
        trace.insert(
            event_path!("dropped_links_count"),
            Value::from(span.dropped_links_count),
        );
        trace.insert(event_path!("status"), Value::from(span.status));
        if let Some(resource) = self.resource {
            if !resource.attributes.is_empty() {
                trace.insert(
                    event_path!(RESOURCE_KEY),
                    kv_list_into_value(resource.attributes),
                );
            }
        }
        trace.insert(event_path!("ingest_timestamp"), Value::from(now));
        trace.into()
    }
}

impl From<SpanEvent> for Value {
    fn from(ev: SpanEvent) -> Self {
        let mut obj: BTreeMap<KeyString, Value> = BTreeMap::new();
        obj.insert("name".into(), ev.name.into());
        obj.insert(
            "time_unix_nano".into(),
            Value::Timestamp(Utc.timestamp_nanos(ev.time_unix_nano as i64)),
        );
        obj.insert("attributes".into(), kv_list_into_value(ev.attributes));
        obj.insert(
            "dropped_attributes_count".into(),
            Value::Integer(ev.dropped_attributes_count as i64),
        );
        Value::Object(obj)
    }
}

impl From<Link> for Value {
    fn from(link: Link) -> Self {
        let mut obj: BTreeMap<KeyString, Value> = BTreeMap::new();
        obj.insert("trace_id".into(), Value::from(to_hex(&link.trace_id)));
        obj.insert("span_id".into(), Value::from(to_hex(&link.span_id)));
        obj.insert("trace_state".into(), link.trace_state.into());
        obj.insert("attributes".into(), kv_list_into_value(link.attributes));
        obj.insert(
            "dropped_attributes_count".into(),
            Value::Integer(link.dropped_attributes_count as i64),
        );
        Value::Object(obj)
    }
}

impl From<SpanStatus> for Value {
    fn from(status: SpanStatus) -> Self {
        let mut obj: BTreeMap<KeyString, Value> = BTreeMap::new();
        obj.insert("message".into(), status.message.into());
        obj.insert("code".into(), status.code.into());
        Value::Object(obj)
    }
}
