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

/// Safely convert nanosecond timestamp (u64) to Value::Timestamp.
/// Returns Value::Null if the value is 0 (unset per OTLP spec) or overflows i64 (past year 2262).
fn nanos_to_value(ns: u64) -> Value {
    if ns == 0 {
        return Value::Null;
    }
    i64::try_from(ns)
        .ok()
        .map(|n| Value::from(Utc.timestamp_nanos(n)))
        .unwrap_or(Value::Null)
}

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
            nanos_to_value(span.start_time_unix_nano),
        );
        trace.insert(
            event_path!("end_time_unix_nano"),
            nanos_to_value(span.end_time_unix_nano),
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

        // Scope-level schema_url (from ScopeSpans). The schema_url field is defined on
        // ScopeSpans (and ResourceSpans) in the OTLP proto and identifies the schema that
        // applies to the instrumentation scope's telemetry:
        // https://github.com/open-telemetry/opentelemetry-proto/blob/v1.0.0/opentelemetry/proto/trace/v1/trace.proto#L73-L74

        if !self.scope_schema_url.is_empty() {
            trace.insert(
                event_path!("scope", "schema_url"),
                Value::from(self.scope_schema_url),
            );
        }

        // Resource-level schema_url (from ResourceSpans). Identifies the schema for the
        // Resource's semantic conventions, distinct from the ScopeSpans schema_url:
        // https://github.com/open-telemetry/opentelemetry-proto/blob/v1.0.0/opentelemetry/proto/trace/v1/trace.proto#L58-L60
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
        obj.insert("time_unix_nano".into(), nanos_to_value(ev.time_unix_nano));
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

    // ========================================================================
    // Tests for InstrumentationScope decode
    // ========================================================================

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

    // ========================================================================
    // Tests for schema_url decode
    // ========================================================================

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

    // ========================================================================
    // Tests for resource.dropped_attributes_count
    // ========================================================================

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

#[cfg(test)]
mod decode_tests {
    use super::*;
    use crate::proto::trace::v1::ScopeSpans;
    use vrl::event_path;

    fn make_resource_spans(span: Span) -> ResourceSpans {
        ResourceSpans {
            resource: None,
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![span],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }
    }

    fn default_span() -> Span {
        Span {
            trace_id: vec![0u8; 16],
            span_id: vec![0u8; 8],
            parent_span_id: Vec::new(),
            trace_state: String::new(),
            name: String::from("test"),
            kind: 0,
            start_time_unix_nano: 0,
            end_time_unix_nano: 0,
            attributes: Vec::new(),
            dropped_attributes_count: 0,
            events: Vec::new(),
            dropped_events_count: 0,
            links: Vec::new(),
            dropped_links_count: 0,
            status: None,
        }
    }

    #[test]
    fn test_zero_span_timestamps_decode_as_null() {
        let span = default_span();
        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        assert_eq!(
            trace.get(event_path!("start_time_unix_nano")),
            Some(&Value::Null),
            "start_time_unix_nano == 0 should decode as Null, not epoch"
        );
        assert_eq!(
            trace.get(event_path!("end_time_unix_nano")),
            Some(&Value::Null),
            "end_time_unix_nano == 0 should decode as Null, not epoch"
        );
    }

    #[test]
    fn test_nonzero_span_timestamps_decode_as_timestamp() {
        let mut span = default_span();
        span.start_time_unix_nano = 1_704_067_200_000_000_000;
        span.end_time_unix_nano = 1_704_067_201_000_000_000;

        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        assert!(
            matches!(
                trace.get(event_path!("start_time_unix_nano")),
                Some(Value::Timestamp(_))
            ),
            "non-zero start_time should decode as Timestamp"
        );
        assert!(
            matches!(
                trace.get(event_path!("end_time_unix_nano")),
                Some(Value::Timestamp(_))
            ),
            "non-zero end_time should decode as Timestamp"
        );
    }

    #[test]
    fn test_zero_span_event_timestamp_decodes_as_null() {
        let mut span = default_span();
        span.events = vec![SpanEvent {
            name: String::from("event0"),
            time_unix_nano: 0,
            attributes: Vec::new(),
            dropped_attributes_count: 0,
        }];

        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        let span_events = trace.get(event_path!("events")).unwrap();
        if let Value::Array(arr) = span_events {
            if let Value::Object(obj) = &arr[0] {
                assert_eq!(
                    obj.get("time_unix_nano"),
                    Some(&Value::Null),
                    "SpanEvent time_unix_nano == 0 should decode as Null"
                );
            } else {
                panic!("Expected Object in events array");
            }
        } else {
            panic!("Expected Array for events");
        }
    }

    #[test]
    fn test_u64_max_span_timestamp() {
        let mut span = default_span();
        span.start_time_unix_nano = u64::MAX;

        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        assert_eq!(
            trace.get(event_path!("start_time_unix_nano")),
            Some(&Value::Null),
            "u64::MAX should decode as Null (overflow protection)"
        );
    }
}

#[cfg(test)]
mod edge_case_tests {
    use super::*;
    use crate::proto::{
        common::v1::{AnyValue, KeyValue, any_value::Value as PBValue},
        trace::v1::ScopeSpans,
    };
    use vrl::event_path;

    fn make_resource_spans_with_scopes(scope_spans: Vec<ScopeSpans>) -> ResourceSpans {
        ResourceSpans {
            resource: None,
            scope_spans,
            schema_url: String::new(),
        }
    }

    fn default_span() -> Span {
        Span {
            trace_id: vec![0u8; 16],
            span_id: vec![0u8; 8],
            parent_span_id: Vec::new(),
            trace_state: String::new(),
            name: String::from("test"),
            kind: 0,
            start_time_unix_nano: 0,
            end_time_unix_nano: 0,
            attributes: Vec::new(),
            dropped_attributes_count: 0,
            events: Vec::new(),
            dropped_events_count: 0,
            links: Vec::new(),
            dropped_links_count: 0,
            status: None,
        }
    }

    fn make_resource_spans(span: Span) -> ResourceSpans {
        ResourceSpans {
            resource: None,
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![span],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }
    }

    #[test]
    fn test_all_span_kinds() {
        for kind in [0, 3, 4, 5] {
            let mut span = default_span();
            span.kind = kind;
            span.name = format!("span-kind-{kind}");

            let rs = make_resource_spans(span);
            let events: Vec<Event> = rs.into_event_iter().collect();
            let trace = events[0].as_trace();

            assert_eq!(
                trace.get(event_path!("kind")),
                Some(&Value::Integer(kind as i64)),
                "kind={kind} should decode correctly"
            );
        }
    }

    #[test]
    fn test_multiple_events_in_span() {
        let ts1 = 1_704_067_200_000_000_000u64;
        let ts2 = 1_704_067_201_000_000_000u64;
        let ts3 = 1_704_067_202_000_000_000u64;

        let mut span = default_span();
        span.events = vec![
            SpanEvent {
                name: String::from("event-a"),
                time_unix_nano: ts1,
                attributes: Vec::new(),
                dropped_attributes_count: 0,
            },
            SpanEvent {
                name: String::from("event-b"),
                time_unix_nano: ts2,
                attributes: vec![KeyValue {
                    key: "key1".to_string(),
                    value: Some(AnyValue {
                        value: Some(PBValue::StringValue("val1".to_string())),
                    }),
                }],
                dropped_attributes_count: 1,
            },
            SpanEvent {
                name: String::from("event-c"),
                time_unix_nano: ts3,
                attributes: Vec::new(),
                dropped_attributes_count: 0,
            },
        ];

        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        let span_events = trace.get(event_path!("events")).unwrap();
        if let Value::Array(arr) = span_events {
            assert_eq!(arr.len(), 3, "should decode all 3 events");
            // Verify names
            for (i, name) in ["event-a", "event-b", "event-c"].iter().enumerate() {
                if let Value::Object(obj) = &arr[i] {
                    assert_eq!(obj.get("name"), Some(&Value::from(*name)));
                }
            }
        } else {
            panic!("Expected Array for events");
        }
    }

    #[test]
    fn test_multiple_links_in_span() {
        let trace_id_1 = hex::decode("0123456789abcdef0123456789abcdef").unwrap();
        let trace_id_2 = hex::decode("fedcba9876543210fedcba9876543210").unwrap();
        let span_id_1 = hex::decode("0123456789abcdef").unwrap();
        let span_id_2 = hex::decode("fedcba9876543210").unwrap();

        let mut span = default_span();
        span.links = vec![
            Link {
                trace_id: trace_id_1,
                span_id: span_id_1,
                trace_state: String::from("vendor=one"),
                attributes: vec![KeyValue {
                    key: "link.kind".to_string(),
                    value: Some(AnyValue {
                        value: Some(PBValue::StringValue("parent".to_string())),
                    }),
                }],
                dropped_attributes_count: 0,
            },
            Link {
                trace_id: trace_id_2,
                span_id: span_id_2,
                trace_state: String::from("vendor=two"),
                attributes: vec![KeyValue {
                    key: "link.reason".to_string(),
                    value: Some(AnyValue {
                        value: Some(PBValue::StringValue("retry".to_string())),
                    }),
                }],
                dropped_attributes_count: 2,
            },
        ];

        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        let links = trace.get(event_path!("links")).unwrap();
        if let Value::Array(arr) = links {
            assert_eq!(arr.len(), 2, "should decode both links");
        } else {
            panic!("Expected Array for links");
        }
    }

    #[test]
    fn test_multiple_spans_per_scope() {
        let mut span_a = default_span();
        span_a.name = String::from("span-a");
        span_a.trace_id = hex::decode("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();

        let mut span_b = default_span();
        span_b.name = String::from("span-b");
        span_b.trace_id = hex::decode("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();

        let mut span_c = default_span();
        span_c.name = String::from("span-c");
        span_c.trace_id = hex::decode("cccccccccccccccccccccccccccccccc").unwrap();

        let rs = make_resource_spans_with_scopes(vec![ScopeSpans {
            scope: Some(InstrumentationScope {
                name: String::from("my-tracer"),
                version: String::from("1.0"),
                attributes: Vec::new(),
                dropped_attributes_count: 0,
            }),
            spans: vec![span_a, span_b, span_c],
            schema_url: String::new(),
        }]);

        let events: Vec<Event> = rs.into_event_iter().collect();
        assert_eq!(
            events.len(),
            3,
            "should decode all 3 spans from single scope"
        );

        let names: Vec<String> = events
            .iter()
            .map(|e| {
                e.as_trace()
                    .get(event_path!("name"))
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert!(names.contains(&"span-a".to_string()));
        assert!(names.contains(&"span-b".to_string()));
        assert!(names.contains(&"span-c".to_string()));
    }

    #[test]
    fn test_status_unset() {
        let mut span = default_span();
        span.status = Some(SpanStatus {
            message: String::new(),
            code: 0,
        });

        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        let status = trace.get(event_path!("status")).unwrap();
        if let Value::Object(obj) = status {
            assert_eq!(obj.get("code"), Some(&Value::Integer(0)));
        } else {
            panic!("Expected Object for status");
        }
    }

    #[test]
    fn test_status_error() {
        let mut span = default_span();
        span.status = Some(SpanStatus {
            message: String::from("deadline exceeded"),
            code: 2,
        });

        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        let status = trace.get(event_path!("status")).unwrap();
        if let Value::Object(obj) = status {
            assert_eq!(obj.get("code"), Some(&Value::Integer(2)));
            assert_eq!(obj.get("message"), Some(&Value::from("deadline exceeded")));
        } else {
            panic!("Expected Object for status");
        }
    }

    #[test]
    fn test_missing_status() {
        let mut span = default_span();
        span.status = None;

        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        let status = trace.get(event_path!("status"));
        assert!(
            status.is_none() || matches!(status, Some(Value::Null)),
            "missing status should decode as None or Null, got {status:?}"
        );
    }

    #[test]
    fn test_trace_state_preservation() {
        let mut span = default_span();
        span.trace_state = String::from("key1=value1,key2=value2");

        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        assert_eq!(
            trace.get(event_path!("trace_state")),
            Some(&Value::from("key1=value1,key2=value2")),
        );
    }

    #[test]
    fn test_unicode_in_span_name() {
        let unicode_name = "処理 /api/注文";

        let mut span = default_span();
        span.name = String::from(unicode_name);

        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        assert_eq!(
            trace.get(event_path!("name")),
            Some(&Value::from(unicode_name)),
        );
    }

    #[test]
    fn test_start_after_end_timestamp() {
        let mut span = default_span();
        span.start_time_unix_nano = 1_704_067_202_000_000_000;
        span.end_time_unix_nano = 1_704_067_200_000_000_000;

        let rs = make_resource_spans(span);
        let events: Vec<Event> = rs.into_event_iter().collect();
        let trace = events[0].as_trace();

        assert!(
            matches!(
                trace.get(event_path!("start_time_unix_nano")),
                Some(Value::Timestamp(_))
            ),
            "start_time should decode even when > end_time"
        );
        assert!(
            matches!(
                trace.get(event_path!("end_time_unix_nano")),
                Some(Value::Timestamp(_))
            ),
            "end_time should decode even when < start_time"
        );
    }
}
