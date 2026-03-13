use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use vector_core::event::{Event, TraceEvent};
use vrl::{
    event_path,
    value::{KeyString, Value},
};

use super::{
    common::{kv_list_into_value, to_hex},
    proto::{
        common::v1::InstrumentationScope,
        resource::v1::Resource,
        trace::v1::{
            ResourceSpans, Span, Status as SpanStatus,
            span::{Event as SpanEvent, Link},
        },
    },
};

pub const TRACE_ID_KEY: &str = "trace_id";
pub const SPAN_ID_KEY: &str = "span_id";
pub const DROPPED_ATTRIBUTES_COUNT_KEY: &str = "dropped_attributes_count";
pub const RESOURCE_KEY: &str = "resources";
pub const ATTRIBUTES_KEY: &str = "attributes";

impl ResourceSpans {
    pub fn into_event_iter(self) -> impl Iterator<Item = Event> {
        let resource = self.resource;
        let resource_schema_url = self.schema_url;
        let now = Utc::now();

        self.scope_spans.into_iter().flat_map(move |scope_span| {
            let scope = scope_span.scope;
            let scope_schema_url = scope_span.schema_url;
            let resource = resource.clone();
            let resource_schema_url = resource_schema_url.clone();
            scope_span.spans.into_iter().map(move |span| {
                ResourceSpan {
                    resource: resource.clone(),
                    scope: scope.clone(),
                    span,
                    scope_schema_url: scope_schema_url.clone(),
                    resource_schema_url: resource_schema_url.clone(),
                }
                .into_event(now)
            })
        })
    }
}

struct ResourceSpan {
    resource: Option<Resource>,
    scope: Option<InstrumentationScope>,
    span: Span,
    scope_schema_url: String,
    resource_schema_url: String,
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
        // Insert instrumentation scope
        if let Some(scope) = self.scope {
            if !scope.name.is_empty() {
                trace.insert(event_path!("scope", "name"), Value::from(scope.name));
            }
            if !scope.version.is_empty() {
                trace.insert(event_path!("scope", "version"), Value::from(scope.version));
            }
            if !scope.attributes.is_empty() {
                trace.insert(
                    event_path!("scope", ATTRIBUTES_KEY),
                    kv_list_into_value(scope.attributes),
                );
            }
            if scope.dropped_attributes_count > 0 {
                trace.insert(
                    event_path!("scope", DROPPED_ATTRIBUTES_COUNT_KEY),
                    Value::from(scope.dropped_attributes_count),
                );
            }
        }

        // Scope-level schema_url (from ScopeSpans)
        if !self.scope_schema_url.is_empty() {
            trace.insert(
                event_path!("scope", "schema_url"),
                Value::from(self.scope_schema_url),
            );
        }

        // Resource-level schema_url (from ResourceSpans)
        if !self.resource_schema_url.is_empty() {
            trace.insert(
                event_path!("schema_url"),
                Value::from(self.resource_schema_url),
            );
        }

        if let Some(resource) = self.resource {
            if !resource.attributes.is_empty() {
                trace.insert(
                    event_path!(RESOURCE_KEY),
                    kv_list_into_value(resource.attributes),
                );
            }
            if resource.dropped_attributes_count > 0 {
                trace.insert(
                    event_path!("resource_dropped_attributes_count"),
                    Value::from(resource.dropped_attributes_count),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{
        common::v1::{AnyValue, KeyValue, any_value::Value as PBValue},
        trace::v1::ScopeSpans,
    };

    fn make_kv(key: &str, val: &str) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue {
                value: Some(PBValue::StringValue(val.to_string())),
            }),
        }
    }

    fn default_span() -> Span {
        Span {
            trace_id: vec![0u8; 16],
            span_id: vec![0u8; 8],
            trace_state: String::new(),
            parent_span_id: vec![],
            name: "test-span".to_string(),
            kind: 1,
            start_time_unix_nano: 1_000_000_000,
            end_time_unix_nano: 2_000_000_000,
            attributes: vec![],
            dropped_attributes_count: 0,
            events: vec![],
            dropped_events_count: 0,
            links: vec![],
            dropped_links_count: 0,
            status: None,
        }
    }

    fn make_resource_spans(
        resource_attrs: Vec<KeyValue>,
        resource_dropped: u32,
        scope: Option<InstrumentationScope>,
        scope_schema_url: &str,
        resource_schema_url: &str,
        span: Span,
    ) -> ResourceSpans {
        ResourceSpans {
            resource: Some(Resource {
                attributes: resource_attrs,
                dropped_attributes_count: resource_dropped,
            }),
            scope_spans: vec![ScopeSpans {
                scope,
                spans: vec![span],
                schema_url: scope_schema_url.to_string(),
            }],
            schema_url: resource_schema_url.to_string(),
        }
    }

    //
    // Tests for InstrumentationScope decode
    //

    #[test]
    fn test_scope_name_version_decoded() {
        let scope = InstrumentationScope {
            name: "my-tracer".to_string(),
            version: "1.0.0".to_string(),
            attributes: vec![],
            dropped_attributes_count: 0,
        };
        let rs = make_resource_spans(vec![], 0, Some(scope), "", "", default_span());
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();
        assert_eq!(
            trace
                .get(event_path!("scope", "name"))
                .unwrap()
                .to_string_lossy(),
            "my-tracer"
        );
        assert_eq!(
            trace
                .get(event_path!("scope", "version"))
                .unwrap()
                .to_string_lossy(),
            "1.0.0"
        );
    }

    #[test]
    fn test_scope_attributes_decoded() {
        let scope = InstrumentationScope {
            name: "tracer".to_string(),
            version: String::new(),
            attributes: vec![make_kv("lib.lang", "rust")],
            dropped_attributes_count: 0,
        };
        let rs = make_resource_spans(vec![], 0, Some(scope), "", "", default_span());
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();
        assert!(trace.get(event_path!("scope", "attributes")).is_some());
    }

    #[test]
    fn test_scope_dropped_attributes_count_decoded() {
        let scope = InstrumentationScope {
            name: "tracer".to_string(),
            version: String::new(),
            attributes: vec![],
            dropped_attributes_count: 3,
        };
        let rs = make_resource_spans(vec![], 0, Some(scope), "", "", default_span());
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();
        assert_eq!(
            *trace
                .get(event_path!("scope", "dropped_attributes_count"))
                .unwrap(),
            Value::Integer(3)
        );
    }

    #[test]
    fn test_no_scope_no_fields_inserted() {
        let rs = make_resource_spans(vec![], 0, None, "", "", default_span());
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();
        assert!(trace.get(event_path!("scope", "name")).is_none());
        assert!(trace.get(event_path!("scope", "version")).is_none());
    }

    //
    // Tests for schema_url decode
    //

    #[test]
    fn test_scope_schema_url_decoded() {
        let rs = make_resource_spans(
            vec![],
            0,
            None,
            "https://opentelemetry.io/schemas/1.21.0",
            "",
            default_span(),
        );
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();
        assert_eq!(
            trace
                .get(event_path!("scope", "schema_url"))
                .unwrap()
                .to_string_lossy(),
            "https://opentelemetry.io/schemas/1.21.0"
        );
    }

    #[test]
    fn test_resource_schema_url_decoded() {
        let rs = make_resource_spans(
            vec![],
            0,
            None,
            "",
            "https://opentelemetry.io/schemas/1.20.0",
            default_span(),
        );
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();
        assert_eq!(
            trace
                .get(event_path!("schema_url"))
                .unwrap()
                .to_string_lossy(),
            "https://opentelemetry.io/schemas/1.20.0"
        );
    }

    #[test]
    fn test_empty_schema_urls_not_inserted() {
        let rs = make_resource_spans(vec![], 0, None, "", "", default_span());
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();
        assert!(trace.get(event_path!("scope", "schema_url")).is_none());
        assert!(trace.get(event_path!("schema_url")).is_none());
    }

    //
    // Tests for resource.dropped_attributes_count
    //

    #[test]
    fn test_resource_dropped_attributes_count() {
        let rs = make_resource_spans(
            vec![make_kv("host.name", "server")],
            7,
            None,
            "",
            "",
            default_span(),
        );
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();
        assert_eq!(
            *trace
                .get(event_path!("resource_dropped_attributes_count"))
                .unwrap(),
            Value::Integer(7)
        );
    }

    #[test]
    fn test_resource_dropped_zero_not_inserted() {
        let rs = make_resource_spans(
            vec![make_kv("host.name", "server")],
            0,
            None,
            "",
            "",
            default_span(),
        );
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();
        assert!(
            trace
                .get(event_path!("resource_dropped_attributes_count"))
                .is_none()
        );
    }

    //
    // Combined: all new fields
    //

    #[test]
    fn test_all_new_fields_together() {
        let scope = InstrumentationScope {
            name: "tracer-lib".to_string(),
            version: "3.0.0".to_string(),
            attributes: vec![make_kv("scope.key", "scope.val")],
            dropped_attributes_count: 2,
        };
        let rs = make_resource_spans(
            vec![make_kv("host.name", "prod-1")],
            4,
            Some(scope),
            "https://scope.schema",
            "https://resource.schema",
            default_span(),
        );
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        // Scope
        assert_eq!(
            trace
                .get(event_path!("scope", "name"))
                .unwrap()
                .to_string_lossy(),
            "tracer-lib"
        );
        assert_eq!(
            trace
                .get(event_path!("scope", "version"))
                .unwrap()
                .to_string_lossy(),
            "3.0.0"
        );
        assert_eq!(
            *trace
                .get(event_path!("scope", "dropped_attributes_count"))
                .unwrap(),
            Value::Integer(2)
        );

        // Schema URLs
        assert_eq!(
            trace
                .get(event_path!("scope", "schema_url"))
                .unwrap()
                .to_string_lossy(),
            "https://scope.schema"
        );
        assert_eq!(
            trace
                .get(event_path!("schema_url"))
                .unwrap()
                .to_string_lossy(),
            "https://resource.schema"
        );

        // Resource
        assert_eq!(
            *trace
                .get(event_path!("resource_dropped_attributes_count"))
                .unwrap(),
            Value::Integer(4)
        );
        assert!(trace.get(event_path!(RESOURCE_KEY)).is_some());
    }
}
