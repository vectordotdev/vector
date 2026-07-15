//! OTLP `Value` (from the `opentelemetry` source with `use_otlp_decoding`)
//! → Obstack [`WriteBatch`] rows.
//!
//! The source's OTLP-decoding mode emits one event per request whose payload
//! mirrors the OTLP protobuf with camelCase JSON field names: attributes are
//! arrays of `{key, value: {stringValue|intValue|...}}`, timestamps are
//! numeric (or numeric-strings), and ids are hex/byte strings. This is a 1:1
//! shape with Obstack's `obstack-ingest` decoders, whose semantics are ported
//! here field-for-field (severity→level, resource→labels, metric name
//! mapping + histogram/summary explosion, span structure).

use vrl::value::Value;

use super::proto::{
    Label, Labels, LogRow, SampleRow, ScalarValue, SpanEvent, SpanKind, SpanLink, SpanRow,
    StatusCode, WriteBatch,
};

const RESOURCE_LOGS: &str = "resourceLogs";
const RESOURCE_METRICS: &str = "resourceMetrics";
const RESOURCE_SPANS: &str = "resourceSpans";

/// Which OTLP signal an event carries, by top-level field.
pub fn signal_of(event: &Value) -> Option<Signal> {
    let obj = event.as_object()?;
    if obj.contains_key(RESOURCE_LOGS) {
        Some(Signal::Logs)
    } else if obj.contains_key(RESOURCE_METRICS) {
        Some(Signal::Metrics)
    } else if obj.contains_key(RESOURCE_SPANS) {
        Some(Signal::Traces)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Logs,
    Metrics,
    Traces,
}

/// Append every row in one OTLP-decoded event into `out`.
pub fn decode_event(event: &Value, out: &mut WriteBatch) {
    match signal_of(event) {
        Some(Signal::Logs) => decode_logs(field(event, RESOURCE_LOGS), out),
        Some(Signal::Metrics) => decode_metrics(field(event, RESOURCE_METRICS), out),
        Some(Signal::Traces) => decode_traces(field(event, RESOURCE_SPANS), out),
        None => {}
    }
}

// ----------------------------------------------------------- accessors --

fn field<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.as_object().and_then(|o| o.get(key))
}

fn arr<'a>(v: Option<&'a Value>) -> &'a [Value] {
    match v.and_then(Value::as_array) {
        Some(a) => a,
        None => &[],
    }
}

/// A VRL string (VRL stores strings as bytes).
fn as_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::Bytes(b)) => String::from_utf8_lossy(b).into_owned(),
        Some(Value::Integer(i)) => i.to_string(),
        Some(Value::Float(f)) => f.to_string(),
        Some(Value::Boolean(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// A numeric field that may arrive as an integer or a numeric string
/// (protobuf JSON encodes 64-bit ints as strings).
fn as_i64(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Integer(i)) => *i,
        Some(Value::Float(f)) => f.into_inner() as i64,
        Some(Value::Bytes(b)) => String::from_utf8_lossy(b).parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}

fn as_f64(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Float(f)) => Some(f.into_inner()),
        Some(Value::Integer(i)) => Some(*i as f64),
        Some(Value::Bytes(b)) => String::from_utf8_lossy(b).parse::<f64>().ok(),
        _ => None,
    }
}

/// An id field: proto bytes render as a hex string here already (the
/// reflection decoder hex/utf8-encodes bytes); pass it through unchanged,
/// lowercased, to match Obstack's `hex::encode`.
fn as_id(v: Option<&Value>) -> String {
    let s = as_string(v);
    if s.is_empty() {
        s
    } else {
        s.to_ascii_lowercase()
    }
}

// ------------------------------------------------------------- anyvalue --

/// Render an OTLP `AnyValue` object (`{stringValue|intValue|...}`) as a
/// display string. Mirrors `obstack_ingest::attrs::any_value_to_string`.
fn any_value_to_string(v: &Value) -> String {
    let Some(obj) = v.as_object() else {
        return as_string(Some(v));
    };
    if let Some(s) = obj.get("stringValue") {
        as_string(Some(s))
    } else if let Some(b) = obj.get("boolValue") {
        as_string(Some(b))
    } else if let Some(i) = obj.get("intValue") {
        as_string(Some(i))
    } else if let Some(d) = obj.get("doubleValue") {
        as_string(Some(d))
    } else if let Some(b) = obj.get("bytesValue") {
        as_string(Some(b))
    } else {
        any_value_to_json(v).to_string()
    }
}

/// Typed scalar for span/resource attributes. Mirrors
/// `obstack_ingest::attrs::flatten_one` for the scalar cases.
fn any_value_scalar(v: &Value) -> Option<ScalarValue> {
    let obj = v.as_object()?;
    if let Some(s) = obj.get("stringValue") {
        Some(ScalarValue::Str(as_string(Some(s))))
    } else if let Some(b) = obj.get("boolValue") {
        Some(ScalarValue::Bool(matches!(b, Value::Boolean(true))))
    } else if let Some(i) = obj.get("intValue") {
        Some(ScalarValue::Int(as_i64(Some(i))))
    } else if let Some(d) = obj.get("doubleValue") {
        as_f64(Some(d)).map(ScalarValue::Float)
    } else if let Some(b) = obj.get("bytesValue") {
        Some(ScalarValue::Str(as_string(Some(b))))
    } else {
        None
    }
}

fn any_value_to_json(v: &Value) -> serde_json::Value {
    serde_json::to_value(v).unwrap_or(serde_json::Value::Null)
}

/// Flatten an `attributes` array (`[{key, value:{..}}]`) into typed pairs,
/// nested kvlists dotted. Mirrors `obstack_ingest::attrs::flatten_attrs`.
fn flatten_attrs(attrs: &[Value]) -> Vec<(String, ScalarValue)> {
    let mut out = Vec::with_capacity(attrs.len());
    for kv in attrs {
        let (Some(key), Some(value)) = (field(kv, "key"), field(kv, "value")) else {
            continue;
        };
        flatten_one(&as_string(Some(key)), value, &mut out);
    }
    out
}

fn flatten_one(key: &str, value: &Value, out: &mut Vec<(String, ScalarValue)>) {
    let Some(obj) = value.as_object() else { return };
    if let Some(kv) = obj.get("kvlistValue") {
        for e in arr(field(kv, "values")) {
            if let (Some(k), Some(v)) = (field(e, "key"), field(e, "value")) {
                flatten_one(&format!("{key}.{}", as_string(Some(k))), v, out);
            }
        }
    } else if obj.contains_key("arrayValue") {
        out.push((key.into(), ScalarValue::Str(any_value_to_json(value).to_string())));
    } else if let Some(scalar) = any_value_scalar(value) {
        out.push((key.into(), scalar));
    }
}

fn attr<'a>(attrs: &'a [(String, ScalarValue)], key: &str) -> Option<&'a ScalarValue> {
    attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn service_name(resource_attrs: &[(String, ScalarValue)]) -> String {
    attr(resource_attrs, "service.name")
        .and_then(scalar_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown_service")
        .to_string()
}

fn scalar_str(v: &ScalarValue) -> Option<&str> {
    match v {
        ScalarValue::Str(s) => Some(s),
        _ => None,
    }
}

fn scalar_render(v: &ScalarValue) -> String {
    match v {
        ScalarValue::Str(s) => s.clone(),
        ScalarValue::Int(i) => i.to_string(),
        ScalarValue::Float(f) => f.to_string(),
        ScalarValue::Bool(b) => b.to_string(),
    }
}

fn resource_attrs_of(resource: Option<&Value>) -> Vec<(String, ScalarValue)> {
    flatten_attrs(arr(resource.and_then(|r| field(r, "attributes"))))
}

/// Sanitize an attribute/metric name into a Loki-safe label name.
/// Mirrors `obstack_model::labels::sanitize_label_name`.
fn sanitize_label_name(name: &str) -> String {
    sanitize(name, false)
}

/// Metric names additionally allow `:`. Mirrors
/// `obstack_ingest::otlp::sanitize_metric_name`.
fn sanitize_metric_name(name: &str) -> String {
    sanitize(name, true)
}

fn sanitize(name: &str, allow_colon: bool) -> String {
    let mut out = String::with_capacity(name.len());
    for (i, c) in name.chars().enumerate() {
        let ok = c.is_ascii_alphanumeric() || c == '_' || (allow_colon && c == ':');
        let ok_first = c.is_ascii_alphabetic() || c == '_' || (allow_colon && c == ':');
        if (i == 0 && ok_first) || (i > 0 && ok) {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() { "_".to_string() } else { out }
}

fn now_ns() -> i64 {
    std::time::UNIX_EPOCH
        .elapsed()
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- logs --

fn severity_level(num: i64) -> &'static str {
    match num {
        1..=4 => "trace",
        5..=8 => "debug",
        9..=12 => "info",
        13..=16 => "warn",
        17..=20 => "error",
        21..=24 => "fatal",
        _ => "",
    }
}

fn decode_logs(resource_logs: Option<&Value>, out: &mut WriteBatch) {
    for rl in arr(resource_logs) {
        let resource_attrs = resource_attrs_of(field(rl, "resource"));
        let svc = service_name(&resource_attrs);
        let mut base_labels: Vec<Label> = resource_attrs
            .iter()
            .map(|(k, v)| Label::new(sanitize_label_name(k), scalar_render(v)))
            .collect();
        base_labels.push(Label::new("service_name", svc));
        for sl in arr(field(rl, "scopeLogs")) {
            for rec in arr(field(sl, "logRecords")) {
                let mut labels = base_labels.clone();
                let sev_text = as_string(field(rec, "severityText"));
                let level = if !sev_text.is_empty() {
                    sev_text.to_lowercase()
                } else {
                    severity_level(as_i64(field(rec, "severityNumber"))).to_string()
                };
                if !level.is_empty() {
                    labels.push(Label::new("level", level));
                }
                let ts = {
                    let t = as_i64(field(rec, "timeUnixNano"));
                    if t > 0 {
                        t
                    } else {
                        let obs = as_i64(field(rec, "observedTimeUnixNano"));
                        if obs > 0 { obs } else { now_ns() }
                    }
                };
                let line = match field(rec, "body") {
                    Some(body) => {
                        if let Some(s) = body.as_object().and_then(|o| o.get("stringValue")) {
                            as_string(Some(s))
                        } else {
                            any_value_to_string(body)
                        }
                    }
                    None => String::new(),
                };
                let mut metadata: Vec<Label> = flatten_attrs(arr(field(rec, "attributes")))
                    .iter()
                    .map(|(k, v)| Label::new(sanitize_label_name(k), scalar_render(v)))
                    .collect();
                let trace_id = as_id(field(rec, "traceId"));
                if !trace_id.is_empty() {
                    metadata.push(Label::new("trace_id", trace_id));
                }
                let span_id = as_id(field(rec, "spanId"));
                if !span_id.is_empty() {
                    metadata.push(Label::new("span_id", span_id));
                }
                out.logs.push((
                    Labels::new(labels),
                    LogRow {
                        fingerprint: Default::default(),
                        timestamp_ns: ts,
                        line,
                        metadata: Labels::new(metadata),
                    },
                ));
            }
        }
    }
}

// -------------------------------------------------------------- traces --

fn decode_traces(resource_spans: Option<&Value>, out: &mut WriteBatch) {
    for rs in arr(resource_spans) {
        let resource_attrs = resource_attrs_of(field(rs, "resource"));
        let svc = service_name(&resource_attrs);
        for ss in arr(field(rs, "scopeSpans")) {
            let scope = field(ss, "scope");
            let scope_name = as_string(scope.and_then(|s| field(s, "name")));
            let scope_version = as_string(scope.and_then(|s| field(s, "version")));
            for span in arr(field(ss, "spans")) {
                let start_ns = as_i64(field(span, "startTimeUnixNano"));
                let end_ns = as_i64(field(span, "endTimeUnixNano"));
                let parent = as_id(field(span, "parentSpanId"));
                let status = field(span, "status");
                out.spans.push(SpanRow {
                    trace_id: as_id(field(span, "traceId")),
                    span_id: as_id(field(span, "spanId")),
                    parent_span_id: (!parent.is_empty()).then_some(parent),
                    name: as_string(field(span, "name")),
                    kind: SpanKind::from_i32(as_i64(field(span, "kind")) as i32),
                    service_name: svc.clone(),
                    start_ns,
                    duration_ns: (end_ns - start_ns).max(0),
                    status: status
                        .map(|s| StatusCode::from_i32(as_i64(field(s, "code")) as i32))
                        .unwrap_or_default(),
                    status_message: as_string(status.and_then(|s| field(s, "message"))),
                    span_attrs: flatten_attrs(arr(field(span, "attributes"))),
                    resource_attrs: resource_attrs.clone(),
                    events: arr(field(span, "events"))
                        .iter()
                        .map(|e| SpanEvent {
                            timestamp_ns: as_i64(field(e, "timeUnixNano")),
                            name: as_string(field(e, "name")),
                            attrs: flatten_attrs(arr(field(e, "attributes"))),
                        })
                        .collect(),
                    links: arr(field(span, "links"))
                        .iter()
                        .map(|l| SpanLink {
                            trace_id: as_id(field(l, "traceId")),
                            span_id: as_id(field(l, "spanId")),
                            attrs: flatten_attrs(arr(field(l, "attributes"))),
                        })
                        .collect(),
                    scope_name: scope_name.clone(),
                    scope_version: scope_version.clone(),
                });
            }
        }
    }
}

// ------------------------------------------------------------- metrics --

fn decode_metrics(resource_metrics: Option<&Value>, out: &mut WriteBatch) {
    for rm in arr(resource_metrics) {
        let resource_attrs = resource_attrs_of(field(rm, "resource"));
        let svc = service_name(&resource_attrs);
        let mut resource_labels: Vec<Label> = Vec::new();
        let job = match attr(&resource_attrs, "service.namespace").and_then(scalar_str) {
            Some(ns) if !ns.is_empty() => format!("{ns}/{svc}"),
            _ => svc,
        };
        resource_labels.push(Label::new("job", job));
        if let Some(inst) = attr(&resource_attrs, "service.instance.id").and_then(scalar_str) {
            resource_labels.push(Label::new("instance", inst));
        }
        for sm in arr(field(rm, "scopeMetrics")) {
            for metric in arr(field(sm, "metrics")) {
                let name = sanitize_metric_name(&as_string(field(metric, "name")));
                let mobj = metric.as_object();
                if let Some(g) = mobj.and_then(|o| o.get("gauge")) {
                    for dp in arr(field(g, "dataPoints")) {
                        push_number_dp(out, &resource_labels, &name, dp);
                    }
                } else if let Some(s) = mobj.and_then(|o| o.get("sum")) {
                    let monotonic = matches!(field(s, "isMonotonic"), Some(Value::Boolean(true)));
                    let name = if monotonic && !name.ends_with("_total") {
                        format!("{name}_total")
                    } else {
                        name.clone()
                    };
                    for dp in arr(field(s, "dataPoints")) {
                        push_number_dp(out, &resource_labels, &name, dp);
                    }
                } else if let Some(h) = mobj.and_then(|o| o.get("histogram")) {
                    for dp in arr(field(h, "dataPoints")) {
                        let ts = as_i64(field(dp, "timeUnixNano"));
                        let base = dp_labels(&resource_labels, dp);
                        let bounds = arr(field(dp, "explicitBounds"));
                        let mut cumulative: f64 = 0.0;
                        for (i, count) in arr(field(dp, "bucketCounts")).iter().enumerate() {
                            cumulative += as_f64(Some(count)).unwrap_or(0.0);
                            let le = bounds
                                .get(i)
                                .and_then(|b| as_f64(Some(b)))
                                .map(format_le)
                                .unwrap_or_else(|| "+Inf".to_string());
                            push_sample(
                                out,
                                with_name_and(&base, &format!("{name}_bucket"), Some(("le", &le))),
                                ts,
                                cumulative,
                            );
                        }
                        if let Some(sum) = as_f64(field(dp, "sum")) {
                            push_sample(out, with_name_and(&base, &format!("{name}_sum"), None), ts, sum);
                        }
                        push_sample(
                            out,
                            with_name_and(&base, &format!("{name}_count"), None),
                            ts,
                            as_f64(field(dp, "count")).unwrap_or(0.0),
                        );
                    }
                } else if let Some(h) = mobj.and_then(|o| o.get("exponentialHistogram")) {
                    for dp in arr(field(h, "dataPoints")) {
                        let ts = as_i64(field(dp, "timeUnixNano"));
                        let base = dp_labels(&resource_labels, dp);
                        if let Some(sum) = as_f64(field(dp, "sum")) {
                            push_sample(out, with_name_and(&base, &format!("{name}_sum"), None), ts, sum);
                        }
                        push_sample(
                            out,
                            with_name_and(&base, &format!("{name}_count"), None),
                            ts,
                            as_f64(field(dp, "count")).unwrap_or(0.0),
                        );
                    }
                } else if let Some(s) = mobj.and_then(|o| o.get("summary")) {
                    for dp in arr(field(s, "dataPoints")) {
                        let ts = as_i64(field(dp, "timeUnixNano"));
                        let base = dp_labels(&resource_labels, dp);
                        for q in arr(field(dp, "quantileValues")) {
                            let qs = format!("{}", as_f64(field(q, "quantile")).unwrap_or(0.0));
                            push_sample(
                                out,
                                with_name_and(&base, &name, Some(("quantile", &qs))),
                                ts,
                                as_f64(field(q, "value")).unwrap_or(0.0),
                            );
                        }
                        push_sample(out, with_name_and(&base, &format!("{name}_sum"), None), ts, as_f64(field(dp, "sum")).unwrap_or(0.0));
                        push_sample(
                            out,
                            with_name_and(&base, &format!("{name}_count"), None),
                            ts,
                            as_f64(field(dp, "count")).unwrap_or(0.0),
                        );
                    }
                }
            }
        }
    }
}

fn push_number_dp(out: &mut WriteBatch, resource_labels: &[Label], name: &str, dp: &Value) {
    let value = if let Some(d) = as_f64(field(dp, "asDouble")) {
        d
    } else if let Some(i) = field(dp, "asInt") {
        as_i64(Some(i)) as f64
    } else {
        return;
    };
    let base = dp_labels(resource_labels, dp);
    push_sample(out, with_name_and(&base, name, None), as_i64(field(dp, "timeUnixNano")), value);
}

fn dp_labels(resource_labels: &[Label], dp: &Value) -> Vec<Label> {
    let mut labels = resource_labels.to_vec();
    for (k, v) in flatten_attrs(arr(field(dp, "attributes"))) {
        labels.push(Label::new(sanitize_label_name(&k), scalar_render(&v)));
    }
    labels
}

fn with_name_and(base: &[Label], name: &str, extra: Option<(&str, &str)>) -> Labels {
    let mut labels = base.to_vec();
    labels.push(Label::new("__name__", name));
    if let Some((k, v)) = extra {
        labels.push(Label::new(k, v));
    }
    Labels::new(labels)
}

fn push_sample(out: &mut WriteBatch, labels: Labels, ts: i64, value: f64) {
    out.samples.push((
        labels,
        SampleRow {
            fingerprint: Default::default(),
            timestamp_ns: ts,
            value,
        },
    ));
}

fn format_le(b: f64) -> String {
    if b == b.trunc() && b.abs() < 1e15 {
        format!("{}", b as i64)
    } else {
        format!("{b}")
    }
}

