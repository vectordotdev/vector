//! Encode one typed span into a Datadog wire `Span`.

use std::{collections::BTreeMap, time::Duration};

use chrono::{DateTime, Utc};

use super::{
    dd_proto::{
        self, attribute_any_value::AttributeAnyValueType,
        attribute_array_value::AttributeArrayValueType,
    },
    derive::{derive_resource_name, derive_span_type},
    keys::{self, ERROR_MESSAGE, TID_TAG},
    string::dd_value_to_string,
};
use crate::event::typed_trace::{
    AttrValue, Attributes, Resource, Span, SpanEvent, SpanId, SpanLink, SpanStatus,
    TraceConversionIssue, TraceConversionReporter, TraceId,
};

const NANOS_PER_SECOND: i128 = 1_000_000_000;

/// Encodes `span` as a Datadog wire span belonging to `trace_id`.
///
/// `service` is the event's `Resource.service`, already defaulted to an empty string.
pub(super) fn encode_span(
    trace_id: TraceId,
    service: &str,
    resource: &Resource,
    span: &Span,
    reporter: &mut impl TraceConversionReporter,
) -> dd_proto::Span {
    let (mut meta, mut metrics) = span_maps(resource, span, reporter);
    apply_trace_id_high(&mut meta, &mut metrics, resource, span, trace_id, reporter);
    apply_status(&mut meta, &mut metrics, resource, span, reporter);

    dd_proto::Span {
        service: service.to_owned(),
        name: span.name.clone(),
        resource: span
            .datadog
            .resource_name
            .clone()
            .unwrap_or_else(|| derive_resource_name(span, resource).into_owned()),
        r#type: span
            .datadog
            .span_type
            .as_deref()
            .unwrap_or_else(|| derive_span_type(span, resource))
            .to_owned(),
        trace_id: trace_id.low_u64(),
        span_id: span.span_id.get(),
        parent_id: span.parent_span_id.map_or(0, SpanId::get),
        error: status_error(&span.status),
        start: datetime_to_i64_nanos(span.start_time, reporter),
        duration: duration_to_i64_nanos(span.duration, reporter),
        meta,
        metrics,
        meta_struct: wire_meta_struct(span),
        span_links: span
            .links
            .iter()
            .map(|link| encode_link(link, reporter))
            .collect(),
        span_events: span
            .events
            .iter()
            .map(|event| encode_event(event, reporter))
            .collect(),
    }
}

const fn status_error(status: &SpanStatus) -> i32 {
    match status {
        SpanStatus::Unset | SpanStatus::Ok => 0,
        SpanStatus::Error(_) | SpanStatus::Other(_) => 1,
    }
}

fn span_maps(
    resource: &Resource,
    span: &Span,
    reporter: &mut impl TraceConversionReporter,
) -> (BTreeMap<String, String>, BTreeMap<String, f64>) {
    let mut meta = BTreeMap::new();
    let mut metrics = BTreeMap::new();
    flatten_resource(&mut meta, resource, span, reporter);
    partition_attributes(&mut meta, &mut metrics, &span.attributes, reporter);
    (meta, metrics)
}

fn flatten_resource(
    meta: &mut BTreeMap<String, String>,
    resource: &Resource,
    span: &Span,
    reporter: &mut impl TraceConversionReporter,
) {
    for (key, value) in resource.attributes.iter() {
        let key = key.as_ref();
        if keys::is_unflattened_resource_key(key) {
            continue;
        }
        if span.attributes.get(key).is_some() {
            reporter.report(TraceConversionIssue::AttributeCollision { key });
            continue;
        }
        if matches!(value, AttrValue::Null) {
            reporter.report(TraceConversionIssue::MalformedAttribute);
            continue;
        }
        meta.insert(key.to_owned(), dd_value_to_string(value));
    }
}

fn partition_attributes(
    meta: &mut BTreeMap<String, String>,
    metrics: &mut BTreeMap<String, f64>,
    attributes: &Attributes,
    reporter: &mut impl TraceConversionReporter,
) {
    for (key, value) in attributes.iter() {
        let key = key.as_ref();
        match value {
            AttrValue::Null => reporter.report(TraceConversionIssue::MalformedAttribute),
            AttrValue::Float(value) => {
                metrics.insert(key.to_owned(), *value);
            }
            AttrValue::Integer(value) => {
                metrics.insert(key.to_owned(), integer_metric(*value));
            }
            AttrValue::String(value) => {
                meta.insert(key.to_owned(), value.clone());
            }
            AttrValue::Bytes(value) => {
                meta.insert(key.to_owned(), String::from_utf8_lossy(value).into_owned());
            }
            AttrValue::Bool(_) | AttrValue::Array(_) | AttrValue::Map(_) => {
                meta.insert(key.to_owned(), dd_value_to_string(value));
            }
        }
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Datadog metrics are f64; integer attributes are coerced onto that wire type."
)]
const fn integer_metric(value: i64) -> f64 {
    value as f64
}

fn apply_trace_id_high(
    meta: &mut BTreeMap<String, String>,
    metrics: &mut BTreeMap<String, f64>,
    resource: &Resource,
    span: &Span,
    trace_id: TraceId,
    reporter: &mut impl TraceConversionReporter,
) {
    let present = carries_key(span, resource, meta, metrics, TID_TAG);
    if present {
        reporter.report(TraceConversionIssue::AttributeCollision { key: TID_TAG });
        remove_key(meta, metrics, TID_TAG);
    }
    let high = trace_id.high_u64();
    if high != 0 {
        meta.insert(TID_TAG.to_owned(), format!("{high:016x}"));
    }
}

fn apply_status(
    meta: &mut BTreeMap<String, String>,
    metrics: &mut BTreeMap<String, f64>,
    resource: &Resource,
    span: &Span,
    reporter: &mut impl TraceConversionReporter,
) {
    let message = span.status.message();
    if message.is_empty() {
        return;
    }
    // An equal scalar `error.message` is the Datadog round trip and is not a
    // divergence. A value in another partition, including `meta_struct`, is.
    let meta_matches = meta
        .get(ERROR_MESSAGE)
        .is_some_and(|value| value == message);
    if meta_matches
        && !metrics.contains_key(ERROR_MESSAGE)
        && !span.datadog.meta_struct.contains_key(ERROR_MESSAGE)
    {
        return;
    }
    if carries_key(span, resource, meta, metrics, ERROR_MESSAGE) {
        reporter.report(TraceConversionIssue::AttributeCollision { key: ERROR_MESSAGE });
        remove_key(meta, metrics, ERROR_MESSAGE);
    }
    meta.insert(ERROR_MESSAGE.to_owned(), message.to_owned());
}

fn carries_key(
    span: &Span,
    resource: &Resource,
    meta: &BTreeMap<String, String>,
    metrics: &BTreeMap<String, f64>,
    key: &str,
) -> bool {
    span.attributes.get(key).is_some()
        || resource.attributes.get(key).is_some()
        || meta.contains_key(key)
        || metrics.contains_key(key)
        || span.datadog.meta_struct.contains_key(key)
}

fn wire_meta_struct(span: &Span) -> BTreeMap<String, Vec<u8>> {
    let drop_error_message = !span.status.message().is_empty();
    span.datadog
        .meta_struct
        .iter()
        .filter(|(key, _)| {
            let key = key.as_ref();
            key != TID_TAG && !(drop_error_message && key == ERROR_MESSAGE)
        })
        .map(|(key, value)| (key.as_ref().to_owned(), value.to_vec()))
        .collect()
}

fn remove_key(meta: &mut BTreeMap<String, String>, metrics: &mut BTreeMap<String, f64>, key: &str) {
    meta.remove(key);
    metrics.remove(key);
}

fn encode_link(link: &SpanLink, reporter: &mut impl TraceConversionReporter) -> dd_proto::SpanLink {
    let mut attributes = BTreeMap::new();
    for (key, value) in link.attributes.iter() {
        if matches!(value, AttrValue::Null) {
            reporter.report(TraceConversionIssue::MalformedAttribute);
            continue;
        }
        attributes.insert(key.as_ref().to_owned(), dd_value_to_string(value));
    }
    dd_proto::SpanLink {
        trace_id: link.trace_id.low_u64(),
        trace_id_high: link.trace_id.high_u64(),
        span_id: link.span_id.get(),
        attributes,
        tracestate: link.trace_state.as_str().to_owned(),
        flags: link.flags.bits(),
    }
}

fn encode_event(
    event: &SpanEvent,
    reporter: &mut impl TraceConversionReporter,
) -> dd_proto::SpanEvent {
    dd_proto::SpanEvent {
        time_unix_nano: datetime_to_u64_nanos(event.time, reporter),
        name: event.name.clone(),
        attributes: encode_event_attributes(&event.attributes, reporter),
    }
}

fn encode_event_attributes(
    attributes: &Attributes,
    reporter: &mut impl TraceConversionReporter,
) -> BTreeMap<String, dd_proto::AttributeAnyValue> {
    let mut encoded = BTreeMap::new();
    for (key, value) in attributes.iter() {
        if matches!(value, AttrValue::Null) {
            reporter.report(TraceConversionIssue::MalformedAttribute);
            continue;
        }
        encoded.insert(key.as_ref().to_owned(), encode_any_value(value));
    }
    encoded
}

fn encode_any_value(value: &AttrValue) -> dd_proto::AttributeAnyValue {
    match value {
        AttrValue::String(value) => string_any(value),
        AttrValue::Bool(value) => dd_proto::AttributeAnyValue {
            r#type: AttributeAnyValueType::BoolValue as i32,
            bool_value: *value,
            ..Default::default()
        },
        AttrValue::Integer(value) => dd_proto::AttributeAnyValue {
            r#type: AttributeAnyValueType::IntValue as i32,
            int_value: *value,
            ..Default::default()
        },
        AttrValue::Float(value) => dd_proto::AttributeAnyValue {
            r#type: AttributeAnyValueType::DoubleValue as i32,
            double_value: *value,
            ..Default::default()
        },
        AttrValue::Array(values) => dd_proto::AttributeAnyValue {
            r#type: AttributeAnyValueType::ArrayValue as i32,
            array_value: Some(dd_proto::AttributeArray {
                values: values.iter().map(encode_array_element).collect(),
            }),
            ..Default::default()
        },
        AttrValue::Bytes(_) | AttrValue::Map(_) | AttrValue::Null => {
            string_any(&dd_value_to_string(value))
        }
    }
}

fn encode_array_element(value: &AttrValue) -> dd_proto::AttributeArrayValue {
    match value {
        AttrValue::String(value) => string_element(value),
        AttrValue::Bool(value) => dd_proto::AttributeArrayValue {
            r#type: AttributeArrayValueType::BoolValue as i32,
            bool_value: *value,
            ..Default::default()
        },
        AttrValue::Integer(value) => dd_proto::AttributeArrayValue {
            r#type: AttributeArrayValueType::IntValue as i32,
            int_value: *value,
            ..Default::default()
        },
        AttrValue::Float(value) => dd_proto::AttributeArrayValue {
            r#type: AttributeArrayValueType::DoubleValue as i32,
            double_value: *value,
            ..Default::default()
        },
        AttrValue::Bytes(_) | AttrValue::Array(_) | AttrValue::Map(_) | AttrValue::Null => {
            string_element(&dd_value_to_string(value))
        }
    }
}

fn string_any(value: &str) -> dd_proto::AttributeAnyValue {
    dd_proto::AttributeAnyValue {
        r#type: AttributeAnyValueType::StringValue as i32,
        string_value: value.to_owned(),
        ..Default::default()
    }
}

fn string_element(value: &str) -> dd_proto::AttributeArrayValue {
    dd_proto::AttributeArrayValue {
        r#type: AttributeArrayValueType::StringValue as i32,
        string_value: value.to_owned(),
        ..Default::default()
    }
}

fn datetime_to_i64_nanos(ts: DateTime<Utc>, reporter: &mut impl TraceConversionReporter) -> i64 {
    let nanos = datetime_nanos(ts);
    i64::try_from(nanos).unwrap_or_else(|_| {
        reporter.report(TraceConversionIssue::ClampedTimestamp);
        if nanos < 0 { i64::MIN } else { i64::MAX }
    })
}

fn datetime_to_u64_nanos(ts: DateTime<Utc>, reporter: &mut impl TraceConversionReporter) -> u64 {
    let nanos = datetime_nanos(ts);
    u64::try_from(nanos).unwrap_or_else(|_| {
        reporter.report(TraceConversionIssue::ClampedTimestamp);
        if nanos < 0 { 0 } else { u64::MAX }
    })
}

fn duration_to_i64_nanos(duration: Duration, reporter: &mut impl TraceConversionReporter) -> i64 {
    let nanos = duration.as_nanos();
    i64::try_from(nanos).unwrap_or_else(|_| {
        reporter.report(TraceConversionIssue::ClampedTimestamp);
        i64::MAX
    })
}

fn datetime_nanos(ts: DateTime<Utc>) -> i128 {
    i128::from(ts.timestamp()) * NANOS_PER_SECOND + i128::from(ts.timestamp_subsec_nanos())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::{DateTime, Utc};

    use super::encode_span;
    use crate::{
        event::typed_trace::{
            AttrValue, OtlpSpanFlags, Resource, Span, SpanEvent, SpanKind, SpanLink, SpanStatus,
            TraceId, TraceState,
        },
        sinks::datadog::traces::typed::testutil::{Issues, span_id, trace_id},
    };

    fn encode(
        span: &Span,
        resource: &Resource,
    ) -> (crate::sinks::datadog::traces::dd_proto::Span, Issues) {
        let mut issues = Issues::default();
        let encoded = encode_span(trace_id(0x11), "checkout", resource, span, &mut issues);
        (encoded, issues)
    }

    fn sample_span() -> Span {
        Span::new(span_id(7), "GET /users")
    }

    #[test]
    fn explicit_datadog_names_win_and_empty_strings_are_kept() {
        let mut span = sample_span();
        span.kind = SpanKind::Client;
        span.attributes
            .insert("db.system", AttrValue::String("postgres".into()));
        span.datadog.span_type = Some(String::new());
        span.datadog.resource_name = Some("x".repeat(5001));
        let (encoded, issues) = encode(&span, &Resource::default());
        assert_eq!(encoded.r#type, "");
        assert_eq!(encoded.resource.len(), 5001);
        assert_eq!(encoded.service, "checkout");
        assert_eq!(encoded.name, "GET /users");
        assert_eq!(issues.unexpected.len(), 0);
    }

    #[test]
    fn partitions_attributes_and_flattens_resource_meta() {
        let mut span = sample_span();
        span.attributes
            .insert("env", AttrValue::String("prod".into()));
        span.attributes.insert(
            "bytes",
            AttrValue::Bytes(bytes::Bytes::from_static(&[0xff])),
        );
        span.attributes.insert("n", AttrValue::Integer(3));
        span.attributes.insert("rate", AttrValue::Float(1.5));
        span.attributes.insert("flag", AttrValue::Bool(false));
        span.attributes.insert("skip", AttrValue::Null);
        span.datadog
            .meta_struct
            .insert("blob".into(), bytes::Bytes::from_static(b"\x01"));
        let mut resource = Resource::default();
        resource
            .attributes
            .insert("region", AttrValue::String("us".into()));
        resource.attributes.insert("load", AttrValue::Float(1.5));
        resource
            .attributes
            .insert("service.name", AttrValue::String("other".into()));
        resource
            .attributes
            .insert("container.id", AttrValue::String("cid".into()));
        resource
            .attributes
            .insert("deployment.environment", AttrValue::String("legacy".into()));
        resource.attributes.insert("gone", AttrValue::Null);
        resource
            .attributes
            .insert("env", AttrValue::String("from-resource".into()));

        let (encoded, issues) = encode(&span, &resource);
        assert_eq!(encoded.meta.get("env").map(String::as_str), Some("prod"));
        assert_eq!(
            encoded.meta.get("bytes").map(String::as_str),
            Some("\u{FFFD}")
        );
        assert_eq!(encoded.meta.get("flag").map(String::as_str), Some("false"));
        assert_eq!(encoded.meta.get("region").map(String::as_str), Some("us"));
        assert_eq!(encoded.meta.get("load").map(String::as_str), Some("1.5"));
        assert_eq!(
            encoded
                .meta
                .get("deployment.environment")
                .map(String::as_str),
            Some("legacy")
        );
        assert!(!encoded.meta.contains_key("service.name"));
        assert!(!encoded.meta.contains_key("container.id"));
        assert!(!encoded.meta.contains_key("skip"));
        assert!(!encoded.meta.contains_key("gone"));
        assert!(!encoded.metrics.contains_key("load"));
        assert_eq!(encoded.metrics.get("n").copied(), Some(3.0));
        assert_eq!(encoded.metrics.get("rate").copied(), Some(1.5));
        assert_eq!(
            encoded.meta_struct.get("blob").map(Vec::as_slice),
            Some(&b"\x01"[..])
        );
        assert_eq!(issues.collisions, vec!["env".to_owned()]);
        assert_eq!(issues.malformed_attributes, 2);
        assert_eq!(issues.unexpected.len(), 0);
    }

    #[test]
    fn status_and_trace_id_high_are_sink_owned() {
        let mut span = sample_span();
        span.status = SpanStatus::Error("boom".into());
        span.attributes
            .insert("error.message", AttrValue::Float(1.5));
        span.attributes
            .insert("_dd.p.tid", AttrValue::String("ffff".into()));
        let trace = TraceId::from_datadog(0, 0xabc).expect("non-zero");
        let mut issues = Issues::default();
        let encoded = encode_span(trace, "svc", &Resource::default(), &span, &mut issues);
        assert_eq!(encoded.error, 1);
        assert_eq!(encoded.trace_id, 0);
        assert_eq!(
            encoded.meta.get("_dd.p.tid").map(String::as_str),
            Some("0000000000000abc")
        );
        assert_eq!(
            encoded.meta.get("error.message").map(String::as_str),
            Some("boom")
        );
        assert!(!encoded.metrics.contains_key("error.message"));
        assert!(!encoded.metrics.contains_key("_dd.p.tid"));
        assert_eq!(
            issues.collisions,
            vec!["_dd.p.tid".to_owned(), "error.message".to_owned()]
        );

        span.attributes.remove("_dd.p.tid");
        span.status = SpanStatus::Error(String::new());
        span.attributes
            .insert("error.message", AttrValue::String("kept".into()));
        let (encoded, issues) = encode(&span, &Resource::default());
        assert_eq!(encoded.error, 1);
        assert_eq!(
            encoded.meta.get("error.message").map(String::as_str),
            Some("kept")
        );
        assert!(!encoded.meta.contains_key("_dd.p.tid"));
        assert_eq!(issues.collisions.len(), 0);

        span.status = SpanStatus::from_i32(9, "weird");
        let (encoded, _) = encode(&span, &Resource::default());
        assert_eq!(encoded.error, 1);
        assert_eq!(
            encoded.meta.get("error.message").map(String::as_str),
            Some("weird")
        );

        span.status = SpanStatus::Ok;
        span.attributes
            .insert("error.message", AttrValue::String("left".into()));
        let (encoded, _) = encode(&span, &Resource::default());
        assert_eq!(encoded.error, 0);
        assert_eq!(
            encoded.meta.get("error.message").map(String::as_str),
            Some("left")
        );
    }

    #[test]
    fn equal_status_message_is_quiet_and_sink_keys_leave_meta_struct() {
        let mut span = sample_span();
        span.status = SpanStatus::Error("boom".into());
        span.attributes
            .insert("error.message", AttrValue::String("boom".into()));
        let (encoded, issues) = encode(&span, &Resource::default());
        assert_eq!(
            encoded.meta.get("error.message").map(String::as_str),
            Some("boom")
        );
        assert_eq!(issues.collisions.len(), 0);

        span.datadog.meta_struct.insert(
            "error.message".into(),
            bytes::Bytes::from_static(b"structured"),
        );
        span.datadog
            .meta_struct
            .insert("_dd.p.tid".into(), bytes::Bytes::from_static(b"tid"));
        span.datadog
            .meta_struct
            .insert("blob".into(), bytes::Bytes::from_static(b"keep"));
        let trace = TraceId::from_datadog(1, 0xabc).expect("non-zero");
        let mut issues = Issues::default();
        let encoded = encode_span(trace, "checkout", &Resource::default(), &span, &mut issues);
        assert_eq!(
            encoded.meta.get("_dd.p.tid").map(String::as_str),
            Some("0000000000000abc")
        );
        assert!(!encoded.meta_struct.contains_key("error.message"));
        assert!(!encoded.meta_struct.contains_key("_dd.p.tid"));
        assert_eq!(
            encoded.meta_struct.get("blob").map(Vec::as_slice),
            Some(&b"keep"[..])
        );
        assert_eq!(
            issues.collisions,
            vec!["_dd.p.tid".to_owned(), "error.message".to_owned()]
        );

        let mut kept = sample_span();
        kept.status = SpanStatus::Error(String::new());
        kept.datadog
            .meta_struct
            .insert("error.message".into(), bytes::Bytes::from_static(b"stay"));
        let (encoded, issues) = encode(&kept, &Resource::default());
        assert_eq!(
            encoded.meta_struct.get("error.message").map(Vec::as_slice),
            Some(&b"stay"[..])
        );
        assert_eq!(issues.collisions.len(), 0);
    }

    #[test]
    fn zero_high_half_strips_an_authored_tid() {
        let mut span = sample_span();
        span.attributes
            .insert("_dd.p.tid", AttrValue::String("ffff".into()));
        let (encoded, issues) = encode(&span, &Resource::default());
        assert!(!encoded.meta.contains_key("_dd.p.tid"));
        assert_eq!(issues.collisions, vec!["_dd.p.tid".to_owned()]);
    }

    #[test]
    fn links_and_events_keep_native_types() {
        let mut span = sample_span();
        let link_trace = TraceId::from_datadog(1, 0xdef).expect("non-zero");
        let mut link = SpanLink {
            trace_id: link_trace,
            span_id: span_id(4),
            trace_state: TraceState::from_raw("vendor=1"),
            flags: OtlpSpanFlags::from(0x8000_0001),
            attributes: crate::event::typed_trace::Attributes::new(),
            dropped_attributes_count: crate::event::typed_trace::DroppedCount::ZERO,
        };
        link.attributes.insert("on", AttrValue::Bool(true));
        link.attributes.insert("skip", AttrValue::Null);
        span.links.push(link);
        span.events.push(SpanEvent {
            name: "exception".into(),
            time: DateTime::from_timestamp(2, 0).expect("timestamp"),
            attributes: crate::event::typed_trace::Attributes::new(),
            dropped_attributes_count: crate::event::typed_trace::DroppedCount::ZERO,
        });
        span.events[0]
            .attributes
            .insert("code", AttrValue::Integer(404));
        span.events[0].attributes.insert(
            "nested",
            AttrValue::Array(vec![AttrValue::Map({
                let mut map = crate::event::typed_trace::AttrMap::new();
                map.insert("k".into(), AttrValue::Integer(1));
                map
            })]),
        );
        span.events[0].attributes.insert("skip", AttrValue::Null);

        let (encoded, issues) = encode(&span, &Resource::default());
        let link = &encoded.span_links[0];
        assert_eq!(link.trace_id, 1);
        assert_eq!(link.trace_id_high, 0xdef);
        assert_eq!(link.span_id, 4);
        assert_eq!(link.tracestate, "vendor=1");
        assert_eq!(link.flags, 0x8000_0001);
        assert_eq!(link.attributes.get("on").map(String::as_str), Some("true"));
        assert!(!link.attributes.contains_key("skip"));
        let event = &encoded.span_events[0];
        assert_eq!(event.time_unix_nano, 2_000_000_000);
        assert_eq!(event.name, "exception");
        assert_eq!(event.attributes["code"].int_value, 404);
        assert_eq!(
            event.attributes["nested"]
                .array_value
                .as_ref()
                .unwrap()
                .values[0]
                .string_value,
            r#"{"k":1}"#
        );
        assert!(!event.attributes.contains_key("skip"));
        assert_eq!(issues.malformed_attributes, 2);
        assert_eq!(issues.unexpected.len(), 0);
    }

    #[test]
    fn timestamps_clamp_to_the_datadog_domains() {
        let mut span = sample_span();
        span.start_time = DateTime::from_timestamp(-1, 0).expect("timestamp");
        span.duration = Duration::from_nanos(1500);
        span.events.push(SpanEvent {
            name: "early".into(),
            time: DateTime::from_timestamp(-1, 0).expect("timestamp"),
            attributes: crate::event::typed_trace::Attributes::new(),
            dropped_attributes_count: crate::event::typed_trace::DroppedCount::ZERO,
        });
        let (encoded, issues) = encode(&span, &Resource::default());
        assert_eq!(encoded.start, -1_000_000_000);
        assert_eq!(encoded.duration, 1500);
        assert_eq!(encoded.span_events[0].time_unix_nano, 0);
        assert_eq!(issues.clamped_timestamps, 1);

        span.start_time = DateTime::<Utc>::MAX_UTC;
        span.duration = Duration::from_secs(u64::try_from(i64::MAX).expect("positive"));
        span.events[0].time = DateTime::<Utc>::MAX_UTC;
        let (encoded, issues) = encode(&span, &Resource::default());
        assert_eq!(encoded.start, i64::MAX);
        assert_eq!(encoded.duration, i64::MAX);
        assert_eq!(encoded.span_events[0].time_unix_nano, u64::MAX);
        assert_eq!(issues.clamped_timestamps, 3);
    }

    #[test]
    fn absent_parent_is_wire_zero() {
        let (encoded, _) = encode(&sample_span(), &Resource::default());
        assert_eq!(encoded.parent_id, 0);
        assert_eq!(encoded.span_id, 7);
        assert_eq!(encoded.trace_id, 0x11);
    }
}
