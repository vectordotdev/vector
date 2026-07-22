//! Datadog trace events -> canonical OTLP `resourceSpans`.
//!
//! Vector's Datadog Agent source deliberately preserves the tracer payload,
//! including unsigned IDs represented as signed `i64` values.  Normalize that
//! source shape once, then use the same OTLP decoder and storage mapping as
//! native OTLP traces.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value as JsonValue, json};
use vrl::value::Value;

pub(super) fn normalize(event: &Value, tenant_attribute: &str) -> Result<Option<Value>, String> {
    let Some(spans) = field(event, "spans").and_then(Value::as_array) else {
        return Ok(None);
    };
    if spans.is_empty() {
        return Err("Datadog trace event has no spans".into());
    }

    let tenant = unique_tag(event, spans, tenant_attribute)?
        .ok_or_else(|| format!("Datadog trace requires exactly one {tenant_attribute} tag"))?;
    let high_tid = unique_tag(event, spans, "_dd.p.tid")?
        .map(|value| parse_high_tid(&value))
        .transpose()?;
    let runtime_id = string(field(event, "runtime_id"));
    let app_version = string(field(event, "app_version"));
    let payload_env = string(field(event, "env"));
    let priority = integer(field(event, "priority")).unwrap_or(0);

    let mut groups: BTreeMap<ResourceKey, Vec<JsonValue>> = BTreeMap::new();
    for span in spans {
        let meta = field(span, "meta").and_then(Value::as_object);
        let service =
            nonempty(string(field(span, "service"))).unwrap_or_else(|| "unknown_service".into());
        let instance = nonempty(tag(meta, "runtime-id"))
            .or_else(|| nonempty(tag(meta, "runtime_id")))
            .or_else(|| nonempty(runtime_id.clone()));
        let version = nonempty(tag(meta, "version"))
            .or_else(|| nonempty(tag(meta, "service.version")))
            .or_else(|| nonempty(app_version.clone()));
        let environment = nonempty(tag(meta, "env"))
            .or_else(|| nonempty(tag(meta, "deployment.environment.name")))
            .or_else(|| nonempty(payload_env.clone()));
        let resource = ResourceKey {
            service,
            instance,
            version,
            environment,
            tenant: tenant.clone(),
        };
        groups
            .entry(resource)
            .or_default()
            .push(normalize_span(span, high_tid, priority)?);
    }

    let resource_spans = groups
        .into_iter()
        .map(|(resource, spans)| {
            json!({
                "resource": {"attributes": resource.attributes()},
                "scopeSpans": [{
                    "scope": {"name": "dd-trace", "version": string(field(event, "tracer_version"))},
                    "spans": spans
                }]
            })
        })
        .collect::<Vec<_>>();
    Ok(Some(Value::from(json!({"resourceSpans": resource_spans}))))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ResourceKey {
    service: String,
    instance: Option<String>,
    version: Option<String>,
    environment: Option<String>,
    tenant: String,
}

impl ResourceKey {
    fn attributes(&self) -> Vec<JsonValue> {
        let mut attrs = vec![
            otlp_string("service.name", &self.service),
            otlp_string("obstack.tenant.id", &self.tenant),
        ];
        if let Some(value) = &self.instance {
            attrs.push(otlp_string("service.instance.id", value));
        }
        if let Some(value) = &self.version {
            attrs.push(otlp_string("service.version", value));
        }
        if let Some(value) = &self.environment {
            attrs.push(otlp_string("deployment.environment.name", value));
        }
        attrs
    }
}

fn normalize_span(
    span: &Value,
    event_high: Option<u64>,
    priority: i64,
) -> Result<JsonValue, String> {
    let low = integer(field(span, "trace_id"))
        .ok_or_else(|| "Datadog span has no trace_id".to_string())? as u64;
    let span_id = integer(field(span, "span_id"))
        .ok_or_else(|| "Datadog span has no span_id".to_string())? as u64;
    let parent_id = integer(field(span, "parent_id")).unwrap_or(0) as u64;
    let meta = field(span, "meta").and_then(Value::as_object);
    let span_high = nonempty(tag(meta, "_dd.p.tid"))
        .map(|value| parse_high_tid(&value))
        .transpose()?
        .or(event_high)
        .unwrap_or(0);
    let start_ns = timestamp_ns(field(span, "start"))
        .ok_or_else(|| "Datadog span has no start timestamp".to_string())?;
    let duration_ns = integer(field(span, "duration")).unwrap_or(0).max(0);
    let operation = string(field(span, "name"));
    let resource = string(field(span, "resource"));
    let span_type = string(field(span, "type"));
    let error = integer(field(span, "error")).unwrap_or(0) != 0;

    let mut attrs = Vec::new();
    if !operation.is_empty() {
        attrs.push(otlp_string("datadog.operation.name", &operation));
    }
    if !resource.is_empty() {
        attrs.push(otlp_string("datadog.resource.name", &resource));
    }
    if !span_type.is_empty() {
        attrs.push(otlp_string("datadog.span.type", &span_type));
    }
    attrs.push(otlp_i64("datadog.sampling.priority", priority));

    // `meta` is mostly string tags, but the tracer also folds two structured
    // payloads into it as JSON strings: `events` (span events) and
    // `_dd.span_links` (span links). Lift those into first-class OTLP
    // `events`/`links` so TraceQL and the Tempo trace API expose them, and
    // coerce well-known numeric semantic keys (e.g. `http.status_code`) back to
    // integers -- dd-trace stringifies them on the wire, which would otherwise
    // defeat numeric TraceQL predicates like `span.http.status_code = 500`.
    let mut events = Vec::new();
    let mut links = Vec::new();
    if let Some(meta) = meta {
        for (key, value) in meta {
            match key.as_str() {
                "_dd.p.tid" => {}
                "events" => match parse_span_events(&string(Some(value))) {
                    Some(parsed) => events = parsed,
                    None => attrs.push(otlp_string(key.as_str(), &string(Some(value)))),
                },
                "_dd.span_links" => match parse_span_links(&string(Some(value))) {
                    Some(parsed) => links = parsed,
                    None => attrs.push(otlp_string(key.as_str(), &string(Some(value)))),
                },
                key if numeric_meta_key(key) => {
                    let raw = string(Some(value));
                    match raw.parse::<i64>() {
                        Ok(number) => attrs.push(otlp_i64(key, number)),
                        Err(_) => attrs.push(otlp_string(key, &raw)),
                    }
                }
                key => attrs.push(otlp_string(key, &string(Some(value)))),
            }
        }
    }

    // Datadog `metrics` are numeric tags. Skip the tracer's internal
    // bookkeeping (`_sampling_priority_v1`, `_dd.measured`, `_top_level`, span
    // sampling, ...): the sampling decision already rides on
    // `datadog.sampling.priority`, and the rest is noise in the attribute set.
    if let Some(metrics) = field(span, "metrics").and_then(Value::as_object) {
        for (key, value) in metrics {
            if key.starts_with('_') {
                continue;
            }
            if let Some(value) = number(Some(value)) {
                attrs.push(otlp_f64(&format!("datadog.metric.{}", key.as_str()), value));
            }
        }
    }

    let mut normalized = json!({
        "traceId": format!("{span_high:016x}{low:016x}"),
        "spanId": format!("{span_id:016x}"),
        "name": if resource.is_empty() { operation } else { resource },
        "kind": span_kind(meta, &span_type),
        "startTimeUnixNano": start_ns.to_string(),
        "endTimeUnixNano": start_ns.saturating_add(duration_ns).to_string(),
        "attributes": attrs,
        "flags": if priority > 0 { 1 } else { 0 },
        "status": {
            "code": if error { 2 } else { 0 },
            "message": tag(meta, "error.msg")
        }
    });
    if parent_id != 0 {
        normalized["parentSpanId"] = json!(format!("{parent_id:016x}"));
    }
    if !events.is_empty() {
        normalized["events"] = JsonValue::Array(events);
    }
    if !links.is_empty() {
        normalized["links"] = JsonValue::Array(links);
    }
    Ok(normalized)
}

fn span_kind(meta: Option<&vrl::value::ObjectMap>, span_type: &str) -> i64 {
    // An explicit `span.kind` tag (set by OTel-shaped integrations) always
    // wins; otherwise infer from the Datadog span `type`. Inbound web requests
    // are servers; outbound HTTP and every datastore/cache client type is a
    // client. Anything ambiguous (e.g. `grpc`, `queue`, `custom`) stays
    // unspecified and relies on `span.kind` when the integration sets it.
    match tag(meta, "span.kind").to_ascii_lowercase().as_str() {
        "internal" => 1,
        "server" => 2,
        "client" => 3,
        "producer" => 4,
        "consumer" => 5,
        _ => match span_type {
            "web" => 2,
            "http" | "sql" | "db" | "cassandra" | "redis" | "memcached" | "mongodb"
            | "elasticsearch" | "leveldb" | "dns" | "consul" => 3,
            _ => 0,
        },
    }
}

/// Semantic-convention keys the Datadog tracer emits as strings in `meta` but
/// that OTLP and TraceQL treat as integers. Coerce these back so numeric
/// predicates such as `span.http.status_code = 500` match. dd-trace stringifies
/// `http.status_code` deliberately (see its span formatter), so without this
/// every DD span carries the status code as an unmatchable string.
fn numeric_meta_key(key: &str) -> bool {
    matches!(
        key,
        "http.status_code"
            | "http.response.status_code"
            | "http.request.status_code"
            | "rpc.grpc.status_code"
    )
}

/// Parse the tracer's `meta.events` JSON array into OTLP span events. Returns
/// `None` when the payload is absent or malformed so the caller preserves the
/// raw string instead of silently dropping it.
fn parse_span_events(raw: &str) -> Option<Vec<JsonValue>> {
    let entries = serde_json::from_str::<JsonValue>(raw).ok()?;
    let events = entries
        .as_array()?
        .iter()
        .filter_map(|entry| {
            let object = entry.as_object()?;
            let name = object.get("name").and_then(JsonValue::as_str).unwrap_or("");
            let time = object
                .get("time_unix_nano")
                .and_then(JsonValue::as_i64)
                .unwrap_or(0);
            Some(json!({
                "name": name,
                "timeUnixNano": time.to_string(),
                "attributes": object.get("attributes").map(json_attrs).unwrap_or_default(),
            }))
        })
        .collect();
    Some(events)
}

/// Parse the tracer's `meta._dd.span_links` JSON array into OTLP span links.
/// The tracer emits 32-/16-char lowercase-hex ids; links with unusable ids are
/// skipped. Returns `None` on an absent or malformed payload.
fn parse_span_links(raw: &str) -> Option<Vec<JsonValue>> {
    let entries = serde_json::from_str::<JsonValue>(raw).ok()?;
    let links = entries
        .as_array()?
        .iter()
        .filter_map(|entry| {
            let object = entry.as_object()?;
            let trace_id =
                normalize_link_id(object.get("trace_id").and_then(JsonValue::as_str)?, 32)?;
            let span_id =
                normalize_link_id(object.get("span_id").and_then(JsonValue::as_str)?, 16)?;
            Some(json!({
                "traceId": trace_id,
                "spanId": span_id,
                "attributes": object.get("attributes").map(json_attrs).unwrap_or_default(),
            }))
        })
        .collect();
    Some(links)
}

/// Normalize a span-link id to canonical lowercase hex of `hex_len` characters,
/// or `None` if it is not valid hex.
fn normalize_link_id(raw: &str, hex_len: usize) -> Option<String> {
    let trimmed = raw.strip_prefix("0x").unwrap_or(raw);
    if trimmed.is_empty()
        || trimmed.len() > hex_len
        || !trimmed.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    Some(format!(
        "{:0>width$}",
        trimmed.to_ascii_lowercase(),
        width = hex_len
    ))
}

/// Convert a plain JSON object -- the shape span-event / span-link attributes
/// take in the tracer's JSON -- into an OTLP `[{key, value}]` attribute array.
fn json_attrs(value: &JsonValue) -> Vec<JsonValue> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    object
        .iter()
        .filter_map(|(key, value)| Some(json!({"key": key, "value": json_scalar(value)?})))
        .collect()
}

/// Map a JSON scalar (or array of scalars) to an OTLP `AnyValue`. Nulls and
/// nested objects are dropped.
fn json_scalar(value: &JsonValue) -> Option<JsonValue> {
    match value {
        JsonValue::String(value) => Some(json!({ "stringValue": value })),
        JsonValue::Bool(value) => Some(json!({ "boolValue": value })),
        JsonValue::Number(value) => match value.as_i64() {
            Some(value) => Some(json!({"intValue": value.to_string()})),
            None => value.as_f64().map(|value| json!({ "doubleValue": value })),
        },
        JsonValue::Array(items) => {
            let values = items.iter().filter_map(json_scalar).collect::<Vec<_>>();
            Some(json!({ "arrayValue": { "values": values } }))
        }
        JsonValue::Null | JsonValue::Object(_) => None,
    }
}

/// Resolve a single value for `key` across the payload-level `tags` and every
/// span's `meta`. The Datadog Agent's v1 trace payload carries no event-level
/// tags, so for that path the tenant is recovered from span `meta` -- where the
/// tracer's global `obstack.tenant.id` tag always lands; v2 payloads may also
/// carry it in chunk/payload tags. Conflicting values are rejected.
fn unique_tag(event: &Value, spans: &[Value], key: &str) -> Result<Option<String>, String> {
    let mut values = BTreeSet::new();
    if let Some(value) = nonempty(tag(field(event, "tags").and_then(Value::as_object), key)) {
        values.insert(value);
    }
    for span in spans {
        if let Some(value) = nonempty(tag(field(span, "meta").and_then(Value::as_object), key)) {
            values.insert(value);
        }
    }
    if values.len() > 1 {
        return Err(format!("Datadog trace has conflicting {key} tags"));
    }
    Ok(values.into_iter().next())
}

fn field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_object().and_then(|object| object.get(key))
}

fn tag(object: Option<&vrl::value::ObjectMap>, key: &str) -> String {
    object
        .and_then(|object| object.get(key))
        .map_or_else(String::new, |value| string(Some(value)))
}

fn string(value: Option<&Value>) -> String {
    match value {
        Some(Value::Bytes(value)) => String::from_utf8_lossy(value).into_owned(),
        Some(Value::Integer(value)) => value.to_string(),
        Some(Value::Float(value)) => value.to_string(),
        Some(Value::Boolean(value)) => value.to_string(),
        _ => String::new(),
    }
}

fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn integer(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Integer(value)) => Some(*value),
        Some(Value::Bytes(value)) => String::from_utf8_lossy(value).parse().ok(),
        _ => None,
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Float(value)) => Some(value.into_inner()),
        Some(Value::Integer(value)) => Some(*value as f64),
        Some(Value::Bytes(value)) => String::from_utf8_lossy(value).parse().ok(),
        _ => None,
    }
}

fn timestamp_ns(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Timestamp(value)) => value.timestamp_nanos_opt(),
        value => integer(value),
    }
}

fn parse_high_tid(value: &str) -> Result<u64, String> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.len() != 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("_dd.p.tid must contain exactly 16 hexadecimal characters".into());
    }
    u64::from_str_radix(value, 16).map_err(|error| error.to_string())
}

fn otlp_string(key: &str, value: &str) -> JsonValue {
    json!({"key": key, "value": {"stringValue": value}})
}

fn otlp_i64(key: &str, value: i64) -> JsonValue {
    json!({"key": key, "value": {"intValue": value.to_string()}})
}

fn otlp_f64(key: &str, value: f64) -> JsonValue {
    json!({"key": key, "value": {"doubleValue": value}})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_span(normalized: &Value) -> Value {
        field(normalized, "resourceSpans")
            .and_then(Value::as_array)
            .and_then(|groups| groups.first())
            .and_then(|group| field(group, "scopeSpans"))
            .and_then(Value::as_array)
            .and_then(|scopes| scopes.first())
            .and_then(|scope| field(scope, "spans"))
            .and_then(Value::as_array)
            .and_then(|spans| spans.first())
            .cloned()
            .unwrap()
    }

    fn attr<'a>(span: &'a Value, key: &str) -> Option<&'a Value> {
        field(span, "attributes")
            .and_then(Value::as_array)?
            .iter()
            .find(|attribute| string(field(attribute, "key")) == key)
            .and_then(|attribute| field(attribute, "value"))
    }

    #[test]
    fn reconstructs_unsigned_128_bit_ids_and_canonical_resources() {
        let event = Value::from(json!({
            "runtime_id": "node-1",
            "app_version": "1.2.3",
            "env": "e2e",
            "priority": 1,
            "tags": {"obstack.tenant.id": "tenant-a"},
            "spans": [{
                "service": "checkout",
                "name": "express.request",
                "resource": "GET /checkout",
                "trace_id": -1,
                "span_id": -2,
                "parent_id": 3,
                "start": 1_000_000_000_i64,
                "duration": 20_i64,
                "error": 0,
                "type": "web",
                "meta": {"_dd.p.tid": "0123456789ABCDEF", "http.method": "GET"},
                "metrics": {}
            }]
        }));
        let normalized = normalize(&event, "obstack.tenant.id").unwrap().unwrap();
        let span = first_span(&normalized);
        assert_eq!(
            string(field(&span, "traceId")),
            "0123456789abcdefffffffffffffffff"
        );
        assert_eq!(string(field(&span, "spanId")), "fffffffffffffffe");
        assert_eq!(integer(field(&span, "kind")), Some(2));
    }

    #[test]
    fn rejects_conflicting_tenants() {
        let event = Value::from(json!({
            "tags": {"obstack.tenant.id": "a"},
            "spans": [{"meta": {"obstack.tenant.id": "b"}}]
        }));
        assert!(normalize(&event, "obstack.tenant.id").is_err());
    }

    fn single_span_event(meta: JsonValue, metrics: JsonValue, span_type: &str) -> Value {
        let event = Value::from(json!({
            "tags": {"obstack.tenant.id": "t"},
            "spans": [{
                "service": "checkout",
                "name": "op",
                "resource": "GET /x",
                "trace_id": 1,
                "span_id": 2,
                "start": 1_000_000_000_i64,
                "duration": 5_i64,
                "type": span_type,
                "meta": meta,
                "metrics": metrics
            }]
        }));
        first_span(&normalize(&event, "obstack.tenant.id").unwrap().unwrap())
    }

    #[test]
    fn coerces_known_numeric_meta_keys_to_integers() {
        let span = single_span_event(
            json!({"http.status_code": "500", "http.method": "GET"}),
            json!({}),
            "web",
        );
        // dd-trace stringifies the status code; it must come back as an int so
        // `span.http.status_code = 500` matches, while free-form tags stay strings.
        assert_eq!(
            integer(attr(&span, "http.status_code").and_then(|v| field(v, "intValue"))),
            Some(500)
        );
        assert!(
            attr(&span, "http.status_code")
                .and_then(|v| field(v, "stringValue"))
                .is_none()
        );
        assert_eq!(
            string(attr(&span, "http.method").and_then(|v| field(v, "stringValue"))),
            "GET"
        );
    }

    #[test]
    fn lifts_meta_events_into_otlp_span_events() {
        let span = single_span_event(
            json!({
                "events": r#"[{"name":"exception","time_unix_nano":1700000000000000000,"attributes":{"exception.type":"Error","retries":2,"flaky":true}}]"#
            }),
            json!({}),
            "web",
        );
        let events = field(&span, "events").and_then(Value::as_array).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(string(field(&events[0], "name")), "exception");
        assert_eq!(
            string(field(&events[0], "timeUnixNano")),
            "1700000000000000000"
        );
        let event_attrs = field(&events[0], "attributes")
            .and_then(Value::as_array)
            .unwrap();
        let by_key = |key: &str| {
            event_attrs
                .iter()
                .find(|a| string(field(a, "key")) == key)
                .and_then(|a| field(a, "value"))
                .cloned()
        };
        assert_eq!(
            string(field(&by_key("exception.type").unwrap(), "stringValue")),
            "Error"
        );
        assert_eq!(
            integer(field(&by_key("retries").unwrap(), "intValue")),
            Some(2)
        );
        assert!(
            field(&span, "attributes")
                .and_then(Value::as_array)
                .unwrap()
                .iter()
                .all(|a| string(field(a, "key")) != "events")
        );
    }

    #[test]
    fn lifts_dd_span_links_into_otlp_links() {
        let span = single_span_event(
            json!({
                "_dd.span_links": r#"[{"trace_id":"0123456789ABCDEF0123456789ABCDEF","span_id":"FEDCBA9876543210","attributes":{"link.kind":"child"},"flags":1}]"#
            }),
            json!({}),
            "web",
        );
        let links = field(&span, "links").and_then(Value::as_array).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(
            string(field(&links[0], "traceId")),
            "0123456789abcdef0123456789abcdef"
        );
        assert_eq!(string(field(&links[0], "spanId")), "fedcba9876543210");
        let link_attrs = field(&links[0], "attributes")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(string(field(&link_attrs[0], "key")), "link.kind");
    }

    #[test]
    fn malformed_structured_meta_falls_back_to_string_attribute() {
        let span = single_span_event(json!({"events": "not json"}), json!({}), "web");
        // Rather than drop it, an unparseable payload survives as a raw attribute.
        assert!(field(&span, "events").is_none());
        assert_eq!(
            string(attr(&span, "events").and_then(|v| field(v, "stringValue"))),
            "not json"
        );
    }

    #[test]
    fn drops_internal_metrics_but_keeps_user_metrics() {
        let span = single_span_event(
            json!({}),
            json!({"_sampling_priority_v1": 1, "_dd.measured": 1, "queue.depth": 7}),
            "web",
        );
        assert!(attr(&span, "datadog.metric._sampling_priority_v1").is_none());
        assert!(attr(&span, "datadog.metric._dd.measured").is_none());
        assert!(attr(&span, "datadog.metric.queue.depth").is_some());
    }

    #[test]
    fn infers_client_kind_for_datastore_span_types() {
        let span = single_span_event(json!({}), json!({}), "redis");
        assert_eq!(integer(field(&span, "kind")), Some(3));
        let explicit = single_span_event(json!({"span.kind": "producer"}), json!({}), "redis");
        assert_eq!(integer(field(&explicit, "kind")), Some(4));
    }
}
