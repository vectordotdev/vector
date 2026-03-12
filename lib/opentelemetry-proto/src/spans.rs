use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use tracing::warn;
use vector_core::event::{Event, TraceEvent};
use vrl::{
    event_path,
    value::{KeyString, Value},
};

use super::{
    common::{
        from_hex, kv_list_into_value, to_hex, validate_span_id, validate_trace_id,
        value_object_to_kv_list,
    },
    proto::{
        collector::trace::v1::ExportTraceServiceRequest,
        common::v1::{AnyValue, InstrumentationScope, KeyValue},
        resource::v1::Resource,
        trace::v1::{
            ResourceSpans, ScopeSpans, Span, Status as SpanStatus,
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
/// Returns Value::Null if the value overflows i64 (past year 2262).
fn nanos_to_value(ns: u64) -> Value {
    i64::try_from(ns)
        .ok()
        .map(|n| Value::from(Utc.timestamp_nanos(n)))
        .unwrap_or(Value::Null)
}

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
        if let Some(resource) = self.resource
            && !resource.attributes.is_empty()
        {
            trace.insert(
                event_path!(RESOURCE_KEY),
                kv_list_into_value(resource.attributes),
            );
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
            Value::Integer(i64::from(ev.dropped_attributes_count)),
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

// ============================================================================
// Native Vector TraceEvent → OTLP Conversion
// ============================================================================

/// Convert a native Vector TraceEvent to OTLP ExportTraceServiceRequest.
///
/// This function handles trace events from any source:
/// - OTLP receiver with `use_otlp_decoding: false` (flat decoded OTLP)
/// - Datadog Agent traces
/// - Any other Vector source that produces TraceEvents
/// - User-modified events with potentially invalid data
///
/// Invalid fields are handled gracefully with defaults/warnings, not errors.
pub fn native_trace_to_otlp_request(trace: &TraceEvent) -> ExportTraceServiceRequest {
    let span = build_span_from_native(trace);

    // Scope-level schema_url: decode path stores at "scope.schema_url".
    let scope_schema_url = trace
        .get(event_path!("scope", "schema_url"))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    let scope_spans = ScopeSpans {
        scope: extract_trace_scope(trace),
        spans: vec![span],
        schema_url: scope_schema_url,
    };

    // Resource-level schema_url: decode path stores at root "schema_url".
    let resource_spans = ResourceSpans {
        resource: extract_trace_resource(trace),
        scope_spans: vec![scope_spans],
        schema_url: extract_trace_string(trace, "schema_url"),
    };

    ExportTraceServiceRequest {
        resource_spans: vec![resource_spans],
    }
}

fn build_span_from_native(trace: &TraceEvent) -> Span {
    let mut attributes = extract_trace_kv_attributes(trace, ATTRIBUTES_KEY);
    // Collect non-OTLP fields (e.g., deployment_id, tenant) into attributes
    // to prevent data loss during conversion
    collect_trace_remaining_fields(trace, &mut attributes);

    Span {
        trace_id: extract_trace_id(trace),
        span_id: extract_span_id(trace, SPAN_ID_KEY),
        parent_span_id: extract_span_id(trace, "parent_span_id"),
        trace_state: extract_trace_string(trace, "trace_state"),
        name: extract_trace_string(trace, "name"),
        kind: extract_trace_i32(trace, "kind"),
        start_time_unix_nano: extract_trace_timestamp_nanos(trace, "start_time_unix_nano"),
        end_time_unix_nano: extract_trace_timestamp_nanos(trace, "end_time_unix_nano"),
        attributes,
        dropped_attributes_count: extract_trace_u32(trace, DROPPED_ATTRIBUTES_COUNT_KEY),
        events: extract_trace_span_events(trace),
        dropped_events_count: extract_trace_u32(trace, "dropped_events_count"),
        links: extract_trace_span_links(trace),
        dropped_links_count: extract_trace_u32(trace, "dropped_links_count"),
        status: extract_trace_status(trace),
    }
}

// ============================================================================
// Remaining fields collection for TraceEvent
// ============================================================================

/// Known OTLP span fields that are extracted into specific Span/scope/resource fields.
/// Fields not in this list are collected as additional attributes to prevent data loss.
const KNOWN_OTLP_SPAN_FIELDS: &[&str] = &[
    TRACE_ID_KEY,
    SPAN_ID_KEY,
    "parent_span_id",
    "trace_state",
    "name",
    "kind",
    "start_time_unix_nano",
    "end_time_unix_nano",
    ATTRIBUTES_KEY,
    DROPPED_ATTRIBUTES_COUNT_KEY,
    "events",
    "dropped_events_count",
    "links",
    "dropped_links_count",
    "status",
    RESOURCE_KEY,
    "resource",
    "resource_attributes",
    "scope",
    "schema_url",
    "resource_dropped_attributes_count",
    "ingest_timestamp", // Added by decode path
];

/// Collect event root fields that are not known OTLP span fields and add them as attributes.
/// This prevents data loss for user-added fields (e.g., deployment_id, tenant, environment).
fn collect_trace_remaining_fields(trace: &TraceEvent, existing_attrs: &mut Vec<KeyValue>) {
    let map = trace.as_map();

    for (key, value) in map.iter() {
        let key_str: &str = key;
        if KNOWN_OTLP_SPAN_FIELDS.contains(&key_str) || matches!(value, Value::Null) {
            continue;
        }
        existing_attrs.push(KeyValue {
            key: key_str.to_string(),
            value: Some(AnyValue {
                value: Some(value.clone().into()),
            }),
        });
    }
}

// ============================================================================
// Safe extraction helpers for TraceEvent fields
// ============================================================================

/// Extract a string field from a TraceEvent.
#[inline]
fn extract_trace_string(trace: &TraceEvent, key: &str) -> String {
    match trace.get(event_path!(key)) {
        Some(Value::Bytes(b)) => std::str::from_utf8(b)
            .map(|s| s.to_owned())
            .unwrap_or_else(|_| String::from_utf8_lossy(b).into_owned()),
        Some(Value::Integer(i)) => i.to_string(),
        Some(Value::Float(f)) => f.to_string(),
        Some(Value::Boolean(b)) => if *b { "true" } else { "false" }.to_string(),
        Some(other) => {
            warn!(
                message = "Converting non-string to string.",
                field = key,
                value_type = ?other,
                internal_log_rate_limit = true
            );
            format!("{other:?}")
        }
        None => String::new(),
    }
}

/// Extract an i32 field from a TraceEvent.
#[inline]
fn extract_trace_i32(trace: &TraceEvent, key: &str) -> i32 {
    match trace.get(event_path!(key)) {
        Some(Value::Integer(i)) => {
            let i = *i;
            if i < i32::MIN as i64 || i > i32::MAX as i64 {
                warn!(
                    message = "Value out of i32 range, clamping.",
                    field = key,
                    value = i,
                    internal_log_rate_limit = true
                );
                i.clamp(i32::MIN as i64, i32::MAX as i64) as i32
            } else {
                i as i32
            }
        }
        Some(Value::Bytes(b)) => {
            let s = String::from_utf8_lossy(b);
            s.parse::<i32>().unwrap_or_else(|_| {
                warn!(message = "Could not parse i32 field.", field = key, value = %s, internal_log_rate_limit = true);
                0
            })
        }
        _ => 0,
    }
}

/// Extract a u32 field from a TraceEvent.
#[inline]
fn extract_trace_u32(trace: &TraceEvent, key: &str) -> u32 {
    match trace.get(event_path!(key)) {
        Some(Value::Integer(i)) => {
            let i = *i;
            if i < 0 {
                warn!(
                    message = "Negative value for u32 field, using 0.",
                    field = key,
                    value = i,
                    internal_log_rate_limit = true
                );
                0
            } else if i > u32::MAX as i64 {
                warn!(
                    message = "Value overflow for u32 field.",
                    field = key,
                    value = i,
                    internal_log_rate_limit = true
                );
                u32::MAX
            } else {
                i as u32
            }
        }
        Some(Value::Bytes(b)) => {
            let s = String::from_utf8_lossy(b);
            s.parse::<u32>().unwrap_or(0)
        }
        _ => 0,
    }
}

/// Extract timestamp as nanoseconds from a TraceEvent field.
/// The decode path stores timestamps as Value::Timestamp via Utc.timestamp_nanos().
fn extract_trace_timestamp_nanos(trace: &TraceEvent, key: &str) -> u64 {
    let value = match trace.get(event_path!(key)) {
        Some(v) => v,
        None => return 0,
    };

    match value {
        Value::Timestamp(ts) => ts
            .timestamp_nanos_opt()
            .filter(|&n| n >= 0)
            .map(|n| n as u64)
            .unwrap_or(0),
        Value::Integer(i) => {
            let i = *i;
            if i < 0 {
                warn!(
                    message = "Negative timestamp, using 0.",
                    field = key,
                    value = i,
                    internal_log_rate_limit = true
                );
                return 0;
            }
            if i < 1_000_000_000_000 {
                (i as u64).saturating_mul(1_000_000_000)
            } else if i < 1_000_000_000_000_000 {
                (i as u64).saturating_mul(1_000_000)
            } else if i < 1_000_000_000_000_000_000 {
                (i as u64).saturating_mul(1_000)
            } else {
                i as u64
            }
        }
        Value::Float(f) => {
            let f = f.into_inner();
            if f < 0.0 || f.is_nan() || f.is_infinite() {
                warn!(
                    message = "Invalid float timestamp, using 0.",
                    field = key,
                    internal_log_rate_limit = true
                );
                return 0;
            }
            let nanos = if f < 1e12 {
                f * 1e9
            } else if f < 1e15 {
                f * 1e6
            } else if f < 1e18 {
                f * 1e3
            } else {
                f
            };
            if nanos > u64::MAX as f64 {
                warn!(
                    message = "Float timestamp overflow, using 0.",
                    field = key,
                    internal_log_rate_limit = true
                );
                0
            } else {
                nanos as u64
            }
        }
        Value::Bytes(b) => {
            let s = String::from_utf8_lossy(b);
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| {
                    dt.timestamp_nanos_opt()
                        .filter(|&n| n >= 0)
                        .map(|n| n as u64)
                        .unwrap_or(0)
                })
                .or_else(|_| {
                    s.parse::<i64>().map(|ts| {
                        if ts < 0 {
                            warn!(
                                message = "Negative timestamp string, using 0.",
                                field = key,
                                value = ts,
                                internal_log_rate_limit = true
                            );
                            0
                        } else if ts < 1_000_000_000_000 {
                            (ts as u64).saturating_mul(1_000_000_000)
                        } else if ts < 1_000_000_000_000_000 {
                            (ts as u64).saturating_mul(1_000_000)
                        } else if ts < 1_000_000_000_000_000_000 {
                            (ts as u64).saturating_mul(1_000)
                        } else {
                            ts as u64
                        }
                    })
                })
                .unwrap_or_else(|_| {
                    warn!(
                        message = "Could not parse timestamp string.",
                        field = key,
                        value = %s,
                        internal_log_rate_limit = true
                    );
                    0
                })
        }
        _ => {
            warn!(
                message = "Unexpected timestamp type.",
                field = key,
                internal_log_rate_limit = true
            );
            0
        }
    }
}

/// Extract trace_id with validation.
/// The decode path stores trace_id as a hex string (Value::Bytes).
#[inline]
fn extract_trace_id(trace: &TraceEvent) -> Vec<u8> {
    match trace.get(event_path!(TRACE_ID_KEY)) {
        Some(Value::Bytes(b)) => {
            if b.len() == 16 {
                return b.to_vec();
            }
            let s = match std::str::from_utf8(b) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            validate_trace_id(&from_hex(s))
        }
        Some(Value::Array(arr)) => {
            let mut bytes = Vec::with_capacity(arr.len().min(16));
            for v in arr.iter() {
                if let Value::Integer(i) = v {
                    bytes.push((*i).clamp(0, 255) as u8);
                }
            }
            validate_trace_id(&bytes)
        }
        _ => Vec::new(),
    }
}

/// Extract span_id or parent_span_id with validation.
/// The decode path stores these as hex strings (Value::Bytes).
#[inline]
fn extract_span_id(trace: &TraceEvent, key: &str) -> Vec<u8> {
    match trace.get(event_path!(key)) {
        Some(Value::Bytes(b)) => {
            if b.len() == 8 {
                return b.to_vec();
            }
            let s = match std::str::from_utf8(b) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            validate_span_id(&from_hex(s))
        }
        Some(Value::Array(arr)) => {
            let mut bytes = Vec::with_capacity(arr.len().min(8));
            for v in arr.iter() {
                if let Value::Integer(i) = v {
                    bytes.push((*i).clamp(0, 255) as u8);
                }
            }
            validate_span_id(&bytes)
        }
        _ => Vec::new(),
    }
}

/// Extract attributes as KeyValue list from a TraceEvent.
#[inline]
fn extract_trace_kv_attributes(trace: &TraceEvent, key: &str) -> Vec<KeyValue> {
    match trace.get(event_path!(key)) {
        Some(Value::Object(obj)) => {
            let mut result = Vec::with_capacity(obj.len());
            for (k, v) in obj.iter() {
                if matches!(v, Value::Null) {
                    continue;
                }
                result.push(KeyValue {
                    key: k.to_string(),
                    value: Some(AnyValue {
                        value: Some(v.clone().into()),
                    }),
                });
            }
            result
        }
        Some(Value::Array(arr)) => {
            let mut result = Vec::with_capacity(arr.len());
            for v in arr.iter() {
                if let Value::Object(obj) = v
                    && let Some(key) = obj.get("key").and_then(|v| v.as_str())
                {
                    result.push(KeyValue {
                        key: key.to_string(),
                        value: obj.get("value").map(|v| AnyValue {
                            value: Some(v.clone().into()),
                        }),
                    });
                }
            }
            result
        }
        _ => Vec::new(),
    }
}

/// Extract instrumentation scope from a TraceEvent.
fn extract_trace_scope(trace: &TraceEvent) -> Option<InstrumentationScope> {
    let scope_name = trace
        .get(event_path!("scope", "name"))
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let scope_version = trace
        .get(event_path!("scope", "version"))
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let scope_attrs = match trace.get(event_path!("scope", "attributes")) {
        Some(Value::Object(obj)) => value_object_to_kv_list(obj),
        _ => Vec::new(),
    };

    // Extract scope.dropped_attributes_count (added by decode fix #24905).
    let scope_dropped =
        match trace.get(event_path!("scope", "dropped_attributes_count")) {
            Some(Value::Integer(i)) => {
                let i = *i;
                if i < 0 {
                    0
                } else if i > u32::MAX as i64 {
                    u32::MAX
                } else {
                    i as u32
                }
            }
            _ => 0,
        };

    if scope_name.is_some()
        || scope_version.is_some()
        || !scope_attrs.is_empty()
        || scope_dropped > 0
    {
        Some(InstrumentationScope {
            name: scope_name.unwrap_or_default(),
            version: scope_version.unwrap_or_default(),
            attributes: scope_attrs,
            dropped_attributes_count: scope_dropped,
        })
    } else {
        None
    }
}

/// Extract resource attributes from a TraceEvent.
#[inline]
fn extract_trace_resource(trace: &TraceEvent) -> Option<Resource> {
    const RESOURCE_FIELDS: [&str; 3] = ["resources", "resource", "resource_attributes"];

    for field in RESOURCE_FIELDS {
        if let Some(v) = trace.get(event_path!(field)) {
            let attrs = match v {
                Value::Object(obj) => {
                    let mut result = Vec::with_capacity(obj.len());
                    for (k, v) in obj.iter() {
                        if matches!(v, Value::Null) {
                            continue;
                        }
                        result.push(KeyValue {
                            key: k.to_string(),
                            value: Some(AnyValue {
                                value: Some(v.clone().into()),
                            }),
                        });
                    }
                    result
                }
                Value::Array(arr) => {
                    let mut result = Vec::with_capacity(arr.len());
                    for item in arr.iter() {
                        if let Value::Object(obj) = item
                            && let Some(key) = obj.get("key").and_then(|v| v.as_str())
                        {
                            result.push(KeyValue {
                                key: key.to_string(),
                                value: obj.get("value").map(|v| AnyValue {
                                    value: Some(v.clone().into()),
                                }),
                            });
                        }
                    }
                    result
                }
                _ => continue,
            };

            if !attrs.is_empty() {
                // Extract resource_dropped_attributes_count (added by decode fix #24905).
                let dropped = match trace
                    .get(event_path!("resource_dropped_attributes_count"))
                {
                    Some(Value::Integer(i)) => {
                        let i = *i;
                        if i < 0 {
                            0
                        } else if i > u32::MAX as i64 {
                            u32::MAX
                        } else {
                            i as u32
                        }
                    }
                    _ => 0,
                };

                return Some(Resource {
                    attributes: attrs,
                    dropped_attributes_count: dropped,
                });
            }
        }
    }
    None
}

/// Extract span events from a TraceEvent.
/// The decode path stores events as an Array of Objects, each with:
/// - name: string
/// - time_unix_nano: Timestamp
/// - attributes: Object
/// - dropped_attributes_count: Integer
fn extract_trace_span_events(trace: &TraceEvent) -> Vec<SpanEvent> {
    let arr = match trace.get(event_path!("events")) {
        Some(Value::Array(arr)) => arr,
        _ => return Vec::new(),
    };

    let mut result = Vec::with_capacity(arr.len());
    for item in arr.iter() {
        if let Value::Object(obj) = item {
            let name = obj
                .get("name")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();

            let time_unix_nano = match obj.get("time_unix_nano") {
                Some(Value::Timestamp(ts)) => ts
                    .timestamp_nanos_opt()
                    .filter(|&n| n >= 0)
                    .map(|n| n as u64)
                    .unwrap_or(0),
                Some(Value::Integer(i)) => {
                    let i = *i;
                    if i < 0 {
                        0
                    } else if i < 1_000_000_000_000 {
                        (i as u64).saturating_mul(1_000_000_000)
                    } else if i < 1_000_000_000_000_000 {
                        (i as u64).saturating_mul(1_000_000)
                    } else if i < 1_000_000_000_000_000_000 {
                        (i as u64).saturating_mul(1_000)
                    } else {
                        i as u64
                    }
                }
                _ => 0,
            };

            let attributes = match obj.get("attributes") {
                Some(Value::Object(attrs)) => value_object_to_kv_list(attrs),
                _ => Vec::new(),
            };

            let dropped_attributes_count = match obj.get("dropped_attributes_count") {
                Some(Value::Integer(i)) => {
                    let i = *i;
                    if i < 0 {
                        0
                    } else if i > u32::MAX as i64 {
                        u32::MAX
                    } else {
                        i as u32
                    }
                }
                _ => 0,
            };

            result.push(SpanEvent {
                name,
                time_unix_nano,
                attributes,
                dropped_attributes_count,
            });
        }
    }
    result
}

/// Extract span links from a TraceEvent.
/// The decode path stores links as an Array of Objects, each with:
/// - trace_id: hex string
/// - span_id: hex string
/// - trace_state: string
/// - attributes: Object
/// - dropped_attributes_count: Integer
fn extract_trace_span_links(trace: &TraceEvent) -> Vec<Link> {
    let arr = match trace.get(event_path!("links")) {
        Some(Value::Array(arr)) => arr,
        _ => return Vec::new(),
    };

    let mut result = Vec::with_capacity(arr.len());
    for item in arr.iter() {
        if let Value::Object(obj) = item {
            let trace_id = match obj.get("trace_id") {
                Some(Value::Bytes(b)) => {
                    let s = String::from_utf8_lossy(b);
                    validate_trace_id(&from_hex(&s))
                }
                _ => Vec::new(),
            };

            let span_id = match obj.get("span_id") {
                Some(Value::Bytes(b)) => {
                    let s = String::from_utf8_lossy(b);
                    validate_span_id(&from_hex(&s))
                }
                _ => Vec::new(),
            };

            let trace_state = obj
                .get("trace_state")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();

            let attributes = match obj.get("attributes") {
                Some(Value::Object(attrs)) => value_object_to_kv_list(attrs),
                _ => Vec::new(),
            };

            let dropped_attributes_count = match obj.get("dropped_attributes_count") {
                Some(Value::Integer(i)) => {
                    let i = *i;
                    if i < 0 {
                        0
                    } else if i > u32::MAX as i64 {
                        u32::MAX
                    } else {
                        i as u32
                    }
                }
                _ => 0,
            };

            result.push(Link {
                trace_id,
                span_id,
                trace_state,
                attributes,
                dropped_attributes_count,
            });
        }
    }
    result
}

/// Extract span status from a TraceEvent.
/// The decode path stores status as an Object with: message (string), code (Integer).
fn extract_trace_status(trace: &TraceEvent) -> Option<SpanStatus> {
    match trace.get(event_path!("status")) {
        Some(Value::Object(obj)) => {
            let message = obj
                .get("message")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();

            let code = match obj.get("code") {
                // OTLP StatusCode: 0=Unset, 1=Ok, 2=Error
                Some(Value::Integer(i)) => (*i).clamp(0, 2) as i32,
                _ => 0,
            };

            Some(SpanStatus { message, code })
        }
        _ => None,
    }
}

#[cfg(test)]
mod native_trace_conversion_tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use vector_core::event::{EventMetadata, ObjectMap};
    use vrl::btreemap;

    fn make_trace(fields: ObjectMap) -> TraceEvent {
        TraceEvent::from_parts(fields, EventMetadata::default())
    }

    #[test]
    fn test_empty_trace_produces_valid_otlp() {
        let trace = TraceEvent::default();
        let request = native_trace_to_otlp_request(&trace);

        assert_eq!(request.resource_spans.len(), 1);
        assert_eq!(request.resource_spans[0].scope_spans.len(), 1);
        assert_eq!(request.resource_spans[0].scope_spans[0].spans.len(), 1);
    }

    #[test]
    fn test_basic_trace_fields() {
        let trace = make_trace(btreemap! {
            "trace_id" => "0123456789abcdef0123456789abcdef",
            "span_id" => "0123456789abcdef",
            "name" => "test-span",
            "kind" => 2,
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert_eq!(span.trace_id.len(), 16);
        assert_eq!(span.span_id.len(), 8);
        assert_eq!(span.name, "test-span");
        assert_eq!(span.kind, 2);
    }

    #[test]
    fn test_trace_timestamps() {
        let start_ts = Utc.timestamp_nanos(1_704_067_200_000_000_000);
        let end_ts = Utc.timestamp_nanos(1_704_067_201_000_000_000);

        let trace = make_trace(btreemap! {
            "start_time_unix_nano" => Value::Timestamp(start_ts),
            "end_time_unix_nano" => Value::Timestamp(end_ts),
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert_eq!(span.start_time_unix_nano, 1_704_067_200_000_000_000u64);
        assert_eq!(span.end_time_unix_nano, 1_704_067_201_000_000_000u64);
    }

    #[test]
    fn test_trace_parent_span_id() {
        let trace = make_trace(btreemap! {
            "parent_span_id" => "abcdef0123456789",
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert_eq!(span.parent_span_id.len(), 8);
        // Verify the bytes match expected hex decode
        assert_eq!(
            span.parent_span_id,
            hex::decode("abcdef0123456789").unwrap()
        );
    }

    #[test]
    fn test_trace_state() {
        let trace = make_trace(btreemap! {
            "trace_state" => "key1=value1,key2=value2",
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert_eq!(span.trace_state, "key1=value1,key2=value2");
    }

    #[test]
    fn test_trace_attributes() {
        let mut attrs = ObjectMap::new();
        attrs.insert("http.method".into(), Value::from("GET"));
        attrs.insert("http.status_code".into(), Value::Integer(200));

        let trace = make_trace(btreemap! {
            "attributes" => Value::Object(attrs),
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert_eq!(span.attributes.len(), 2);
        // Verify attribute keys are present
        let keys: Vec<&str> = span.attributes.iter().map(|kv| kv.key.as_str()).collect();
        assert!(keys.contains(&"http.method"));
        assert!(keys.contains(&"http.status_code"));
    }

    #[test]
    fn test_trace_resources() {
        let mut resources = ObjectMap::new();
        resources.insert("service.name".into(), Value::from("test-service"));
        resources.insert("host.name".into(), Value::from("test-host"));

        let trace = make_trace(btreemap! {
            "resources" => Value::Object(resources),
        });

        let request = native_trace_to_otlp_request(&trace);
        let resource = request.resource_spans[0].resource.as_ref().unwrap();

        assert_eq!(resource.attributes.len(), 2);
        let keys: Vec<&str> = resource
            .attributes
            .iter()
            .map(|kv| kv.key.as_str())
            .collect();
        assert!(keys.contains(&"service.name"));
        assert!(keys.contains(&"host.name"));
    }

    #[test]
    fn test_trace_status() {
        let mut status_obj = ObjectMap::new();
        status_obj.insert("message".into(), Value::from("OK"));
        status_obj.insert("code".into(), Value::Integer(1));

        let trace = make_trace(btreemap! {
            "status" => Value::Object(status_obj),
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];
        let status = span.status.as_ref().unwrap();

        assert_eq!(status.message, "OK");
        assert_eq!(status.code, 1);
    }

    #[test]
    fn test_trace_events() {
        let ts = Utc.timestamp_nanos(1_704_067_200_000_000_000);

        let mut event_attrs = ObjectMap::new();
        event_attrs.insert("exception.type".into(), Value::from("RuntimeError"));

        let mut event_obj = ObjectMap::new();
        event_obj.insert("name".into(), Value::from("exception"));
        event_obj.insert("time_unix_nano".into(), Value::Timestamp(ts));
        event_obj.insert("attributes".into(), Value::Object(event_attrs));
        event_obj.insert("dropped_attributes_count".into(), Value::Integer(0));

        let trace = make_trace(btreemap! {
            "events" => Value::Array(vec![Value::Object(event_obj)]),
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert_eq!(span.events.len(), 1);
        assert_eq!(span.events[0].name, "exception");
        assert_eq!(span.events[0].time_unix_nano, 1_704_067_200_000_000_000u64);
        assert_eq!(span.events[0].attributes.len(), 1);
        assert_eq!(span.events[0].attributes[0].key, "exception.type");
    }

    #[test]
    fn test_trace_links() {
        let mut link_attrs = ObjectMap::new();
        link_attrs.insert("link.type".into(), Value::from("parent"));

        let mut link_obj = ObjectMap::new();
        link_obj.insert(
            "trace_id".into(),
            Value::from("0123456789abcdef0123456789abcdef"),
        );
        link_obj.insert("span_id".into(), Value::from("0123456789abcdef"));
        link_obj.insert("trace_state".into(), Value::from("key=value"));
        link_obj.insert("attributes".into(), Value::Object(link_attrs));
        link_obj.insert("dropped_attributes_count".into(), Value::Integer(0));

        let trace = make_trace(btreemap! {
            "links" => Value::Array(vec![Value::Object(link_obj)]),
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert_eq!(span.links.len(), 1);
        assert_eq!(span.links[0].trace_id.len(), 16);
        assert_eq!(span.links[0].span_id.len(), 8);
        assert_eq!(span.links[0].trace_state, "key=value");
        assert_eq!(span.links[0].attributes.len(), 1);
        assert_eq!(span.links[0].attributes[0].key, "link.type");
    }

    #[test]
    fn test_trace_dropped_counts() {
        let trace = make_trace(btreemap! {
            "dropped_attributes_count" => 3,
            "dropped_events_count" => 5,
            "dropped_links_count" => 7,
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert_eq!(span.dropped_attributes_count, 3);
        assert_eq!(span.dropped_events_count, 5);
        assert_eq!(span.dropped_links_count, 7);
    }

    #[test]
    fn test_invalid_trace_id_handled() {
        let trace = make_trace(btreemap! {
            "trace_id" => "not-hex",
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert!(span.trace_id.is_empty());
    }

    #[test]
    fn test_invalid_span_id_handled() {
        let trace = make_trace(btreemap! {
            "span_id" => "not-hex",
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert!(span.span_id.is_empty());
    }

    #[test]
    fn test_wrong_length_trace_id_rejected() {
        // 6 hex chars = 3 bytes, not valid 16-byte trace_id
        let trace = make_trace(btreemap! {
            "trace_id" => "abcdef",
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert!(
            span.trace_id.is_empty(),
            "Wrong-length hex should produce empty trace_id"
        );
    }

    #[test]
    fn test_mixed_valid_invalid_trace_fields() {
        let trace = make_trace(btreemap! {
            "name" => "valid-span",
            "kind" => 1,
            "trace_id" => "not-hex",
            "span_id" => "also-not-hex",
            "dropped_attributes_count" => 2,
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        // Valid fields should be present
        assert_eq!(span.name, "valid-span");
        assert_eq!(span.kind, 1);
        assert_eq!(span.dropped_attributes_count, 2);

        // Invalid fields should have safe defaults
        assert!(span.trace_id.is_empty());
        assert!(span.span_id.is_empty());
    }

    #[test]
    fn test_trace_scope_extraction() {
        let mut scope = ObjectMap::new();
        scope.insert("name".into(), Value::from("my-tracer"));
        scope.insert("version".into(), Value::from("1.2.3"));

        let trace = make_trace(btreemap! {
            "name" => "test-span",
            "scope" => Value::Object(scope),
        });

        let request = native_trace_to_otlp_request(&trace);
        let scope = request.resource_spans[0].scope_spans[0]
            .scope
            .as_ref()
            .unwrap();

        assert_eq!(scope.name, "my-tracer");
        assert_eq!(scope.version, "1.2.3");
    }

    #[test]
    fn test_trace_scope_empty_produces_none() {
        let trace = make_trace(btreemap! {
            "name" => "test-span",
        });

        let request = native_trace_to_otlp_request(&trace);
        assert!(request.resource_spans[0].scope_spans[0].scope.is_none());
    }

    #[test]
    fn test_trace_resource_schema_url() {
        // Root "schema_url" maps to ResourceSpans.schema_url (resource level)
        let trace = make_trace(btreemap! {
            "name" => "test-span",
            "schema_url" => "https://opentelemetry.io/schemas/1.21.0",
        });

        let request = native_trace_to_otlp_request(&trace);
        assert_eq!(
            request.resource_spans[0].schema_url,
            "https://opentelemetry.io/schemas/1.21.0"
        );
    }

    #[test]
    fn test_trace_scope_schema_url() {
        // "scope.schema_url" maps to ScopeSpans.schema_url (scope level)
        let mut trace = TraceEvent::default();
        trace.insert(event_path!("name"), Value::from("test-span"));
        trace.insert(
            event_path!("scope", "schema_url"),
            Value::from("https://scope.schema/1.0"),
        );

        let request = native_trace_to_otlp_request(&trace);
        assert_eq!(
            request.resource_spans[0].scope_spans[0].schema_url,
            "https://scope.schema/1.0"
        );
    }

    #[test]
    fn test_trace_scope_dropped_attributes_count() {
        let mut trace = TraceEvent::default();
        trace.insert(event_path!("name"), Value::from("test-span"));
        trace.insert(event_path!("scope", "name"), Value::from("tracer"));
        trace.insert(
            event_path!("scope", "dropped_attributes_count"),
            Value::Integer(3),
        );

        let request = native_trace_to_otlp_request(&trace);
        let scope = request.resource_spans[0].scope_spans[0]
            .scope
            .as_ref()
            .unwrap();
        assert_eq!(scope.dropped_attributes_count, 3);
    }

    #[test]
    fn test_trace_resource_dropped_attributes_count() {
        let mut trace = TraceEvent::default();
        trace.insert(event_path!("name"), Value::from("test-span"));
        trace.insert(
            event_path!(RESOURCE_KEY),
            kv_list_into_value(vec![KeyValue {
                key: "host.name".to_string(),
                value: Some(AnyValue {
                    value: Some(
                        super::super::proto::common::v1::any_value::Value::StringValue(
                            "server".to_string(),
                        ),
                    ),
                }),
            }]),
        );
        trace.insert(
            event_path!("resource_dropped_attributes_count"),
            Value::Integer(7),
        );

        let request = native_trace_to_otlp_request(&trace);
        let resource = request.resource_spans[0].resource.as_ref().unwrap();
        assert_eq!(resource.dropped_attributes_count, 7);
    }

    #[test]
    fn test_trace_timestamp_as_milliseconds() {
        let trace = make_trace(btreemap! {
            "start_time_unix_nano" => 1704067200000i64,
            "end_time_unix_nano" => 1704067201000i64,
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert_eq!(span.start_time_unix_nano, 1704067200_000_000_000u64);
        assert_eq!(span.end_time_unix_nano, 1704067201_000_000_000u64);
    }

    #[test]
    fn test_trace_timestamp_as_microseconds() {
        let trace = make_trace(btreemap! {
            "start_time_unix_nano" => 1704067200_000_000i64,
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        assert_eq!(span.start_time_unix_nano, 1704067200_000_000_000u64);
    }

    // ========================================================================
    // Remaining fields → attributes tests
    // ========================================================================

    #[test]
    fn test_unknown_trace_fields_collected_as_attributes() {
        let trace = make_trace(btreemap! {
            "name" => "test-span",
            "trace_id" => "0123456789abcdef0123456789abcdef",
            "span_id" => "0123456789abcdef",
            "deployment_id" => "deploy-42",
            "tenant" => "acme-corp",
            "environment" => "production",
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        let attr_keys: Vec<&str> = span.attributes.iter().map(|kv| kv.key.as_str()).collect();
        assert!(
            attr_keys.contains(&"deployment_id"),
            "deployment_id should be in attributes, got {attr_keys:?}"
        );
        assert!(
            attr_keys.contains(&"tenant"),
            "tenant should be in attributes, got {attr_keys:?}"
        );
        assert!(
            attr_keys.contains(&"environment"),
            "environment should be in attributes, got {attr_keys:?}"
        );
    }

    #[test]
    fn test_known_trace_fields_not_in_attributes() {
        let trace = make_trace(btreemap! {
            "name" => "test-span",
            "trace_id" => "0123456789abcdef0123456789abcdef",
            "span_id" => "0123456789abcdef",
            "kind" => 2,
            "trace_state" => "key=value",
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        let attr_keys: Vec<&str> = span.attributes.iter().map(|kv| kv.key.as_str()).collect();
        assert!(
            !attr_keys.contains(&"name"),
            "known field 'name' should not be in attributes"
        );
        assert!(
            !attr_keys.contains(&"trace_id"),
            "known field 'trace_id' should not be in attributes"
        );
        assert!(
            !attr_keys.contains(&"span_id"),
            "known field 'span_id' should not be in attributes"
        );
        assert!(
            !attr_keys.contains(&"kind"),
            "known field 'kind' should not be in attributes"
        );
        assert!(
            !attr_keys.contains(&"trace_state"),
            "known field 'trace_state' should not be in attributes"
        );
    }

    #[test]
    fn test_trace_remaining_fields_merged_with_explicit_attributes() {
        let mut attrs = ObjectMap::new();
        attrs.insert("http.method".into(), Value::from("GET"));

        let trace = make_trace(btreemap! {
            "name" => "http-request",
            "attributes" => Value::Object(attrs),
            "custom_tag" => "my-value",
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        let attr_keys: Vec<&str> = span.attributes.iter().map(|kv| kv.key.as_str()).collect();
        assert!(
            attr_keys.contains(&"http.method"),
            "explicit attribute should be present"
        );
        assert!(
            attr_keys.contains(&"custom_tag"),
            "remaining field should be in attributes"
        );
    }

    #[test]
    fn test_trace_null_fields_not_in_attributes() {
        let trace = make_trace(btreemap! {
            "name" => "test-span",
            "should_be_dropped" => Value::Null,
            "valid_field" => "keep me",
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        let attr_keys: Vec<&str> = span.attributes.iter().map(|kv| kv.key.as_str()).collect();
        assert!(
            !attr_keys.contains(&"should_be_dropped"),
            "Null fields must not appear in attributes"
        );
        assert!(attr_keys.contains(&"valid_field"));
    }

    #[test]
    fn test_trace_many_custom_fields_preserved() {
        use super::super::proto::common::v1::any_value::Value as PBValue;

        let trace = make_trace(btreemap! {
            "name" => "db-query",
            "trace_id" => "0123456789abcdef0123456789abcdef",
            "span_id" => "0123456789abcdef",
            "host" => "db-primary-1",
            "pod_name" => "api-7b9f4d-x2k9p",
            "namespace" => "production",
            "db_latency_ms" => 42i64,
            "is_cached" => false,
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        let attr_keys: Vec<&str> = span.attributes.iter().map(|kv| kv.key.as_str()).collect();
        for expected in ["host", "pod_name", "namespace", "db_latency_ms", "is_cached"] {
            assert!(
                attr_keys.contains(&expected),
                "'{expected}' should be in attributes, got {attr_keys:?}"
            );
        }

        // Verify types preserved
        let find = |key: &str| -> &PBValue {
            span.attributes
                .iter()
                .find(|kv| kv.key == key)
                .unwrap()
                .value
                .as_ref()
                .unwrap()
                .value
                .as_ref()
                .unwrap()
        };

        assert!(matches!(find("db_latency_ms"), PBValue::IntValue(42)));
        assert!(matches!(find("is_cached"), PBValue::BoolValue(false)));
    }

    #[test]
    fn test_trace_ingest_timestamp_not_in_attributes() {
        // ingest_timestamp is added by the decode path and should be treated as known
        let trace = make_trace(btreemap! {
            "name" => "test-span",
            "ingest_timestamp" => Value::Timestamp(Utc::now()),
            "custom_field" => "keep me",
        });

        let request = native_trace_to_otlp_request(&trace);
        let span = &request.resource_spans[0].scope_spans[0].spans[0];

        let attr_keys: Vec<&str> = span.attributes.iter().map(|kv| kv.key.as_str()).collect();
        assert!(
            !attr_keys.contains(&"ingest_timestamp"),
            "ingest_timestamp is a known field, should not be in attributes"
        );
        assert!(attr_keys.contains(&"custom_field"));
    }
}
