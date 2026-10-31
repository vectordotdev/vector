use std::collections::BTreeMap;

use base64::prelude::{BASE64_STANDARD, Engine as _};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use ordered_float::NotNan;
use serde_json::{Map as JsonMap, Value as JsonValue};
use vrl::{event_path, value};

use super::{super::ddtrace_proto, handler::new_trace_event};
use crate::{
    common::datadog::encode_u64_id_hex,
    event::{ObjectMap, TraceEvent, Value},
};
use ddtrace_proto::idx::{
    AnyValue, InvalidStringRef, SpanKind, StringTable, any_value::Value as IdxValue,
    format_sampling_mechanism, span_kind_name, split_trace_id,
};

const SAMPLING_MECHANISM_TAG: &str = "_dd.p.dm";
const TRACE_ID_HIGH_TAG: &str = "_dd.p.tid";

struct IdxTracerFields<'a> {
    tags: &'a ObjectMap,
    container_id: &'a str,
    language_name: &'a str,
    language_version: &'a str,
    tracer_version: &'a str,
    runtime_id: &'a str,
    app_version: &'a str,
}

pub(super) fn convert_idx_tracer_payload(
    payload: ddtrace_proto::idx::TracerPayload,
) -> Result<Vec<TraceEvent>, InvalidStringRef> {
    let strings = StringTable::new(&payload.strings);
    let payload_tags = attributes_as_strings(&strings, &payload.attributes)?;
    let fields = IdxTracerFields {
        tags: &payload_tags,
        container_id: strings.get(payload.container_id_ref)?,
        language_name: strings.get(payload.language_name_ref)?,
        language_version: strings.get(payload.language_version_ref)?,
        tracer_version: strings.get(payload.tracer_version_ref)?,
        runtime_id: strings.get(payload.runtime_id_ref)?,
        app_version: strings.get(payload.app_version_ref)?,
    };

    payload
        .chunks
        .into_iter()
        .map(|chunk| convert_idx_chunk(&strings, &fields, chunk))
        .collect()
}

fn convert_idx_chunk(
    strings: &StringTable<'_>,
    fields: &IdxTracerFields<'_>,
    chunk: ddtrace_proto::idx::TraceChunk,
) -> Result<TraceEvent, InvalidStringRef> {
    let (trace_id_high, trace_id_low) = split_trace_id(&chunk.trace_id);
    let trace_id_high_tag = (trace_id_high != 0).then(|| format!("{trace_id_high:016x}"));

    let mut tags = attributes_as_strings(strings, &chunk.attributes)?;
    if chunk.sampling_mechanism != 0 && !tags.contains_key(SAMPLING_MECHANISM_TAG) {
        tags.insert(
            SAMPLING_MECHANISM_TAG.into(),
            Value::from(format_sampling_mechanism(chunk.sampling_mechanism)),
        );
    }
    tags.extend(fields.tags.clone());

    let mut trace_event = new_trace_event();
    trace_event.insert(event_path!("priority"), i64::from(chunk.priority));
    trace_event.insert(
        event_path!("origin"),
        strings.get(chunk.origin_ref)?.to_owned(),
    );
    trace_event.insert(event_path!("dropped"), chunk.dropped_trace);
    trace_event.insert(event_path!("tags"), Value::from(tags));
    trace_event.insert(
        event_path!("spans"),
        chunk
            .spans
            .into_iter()
            .enumerate()
            .map(|(index, span)| {
                let trace_id_high_tag = trace_id_high_tag.as_deref().filter(|_| index == 0);
                convert_idx_span(strings, span, trace_id_low, trace_id_high_tag).map(Value::from)
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    trace_event.insert(event_path!("container_id"), fields.container_id.to_owned());
    trace_event.insert(
        event_path!("language_name"),
        fields.language_name.to_owned(),
    );
    trace_event.insert(
        event_path!("language_version"),
        fields.language_version.to_owned(),
    );
    trace_event.insert(
        event_path!("tracer_version"),
        fields.tracer_version.to_owned(),
    );
    trace_event.insert(event_path!("runtime_id"), fields.runtime_id.to_owned());
    trace_event.insert(event_path!("app_version"), fields.app_version.to_owned());
    Ok(trace_event)
}

fn convert_idx_span(
    strings: &StringTable<'_>,
    span: ddtrace_proto::idx::Span,
    trace_id_low: u64,
    trace_id_high_tag: Option<&str>,
) -> Result<ObjectMap, InvalidStringRef> {
    let mut attributes = SpanAttributes::from_attributes(strings, span.attributes)?;
    if let Some(trace_id_high_tag) = trace_id_high_tag
        && !attributes.meta.contains_key(TRACE_ID_HIGH_TAG)
    {
        attributes.meta.insert(
            TRACE_ID_HIGH_TAG.into(),
            Value::from(trace_id_high_tag.to_owned()),
        );
    }
    for (key, value_ref) in [
        ("env", span.env_ref),
        ("version", span.version_ref),
        ("component", span.component_ref),
    ] {
        attributes.insert_promoted(key, strings.get(value_ref)?);
    }
    if let Some(kind) = promoted_span_kind(span.kind) {
        attributes.insert_promoted("span.kind", kind);
    }

    let span_links = span
        .links
        .into_iter()
        .map(|link| convert_idx_link(strings, &link).map(Value::from))
        .collect::<Result<Vec<_>, _>>()?;
    let span_events = span
        .events
        .into_iter()
        .map(|event| convert_idx_event(strings, &event).map(Value::from))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(value!({
        service: (strings.get(span.service_ref)?),
        name: (strings.get(span.name_ref)?),
        resource: (strings.get(span.resource_ref)?),
        trace_id: (u64_as_i64(trace_id_low)),
        span_id: (u64_as_i64(span.span_id)),
        parent_id: (u64_as_i64(span.parent_id)),
        start: (Utc.timestamp_nanos(u64_as_i64(span.start))),
        duration: (u64_as_i64(span.duration)),
        error: (i64::from(span.error)),
        meta: (attributes.meta),
        metrics: (attributes.metrics),
        "type": (strings.get(span.type_ref)?),
        meta_struct: (attributes.meta_struct),
        span_links: span_links,
        span_events: span_events,
    })
    .into_object()
    .expect("value! produces an object"))
}

fn convert_idx_link(
    strings: &StringTable<'_>,
    link: &ddtrace_proto::idx::SpanLink,
) -> Result<ObjectMap, InvalidStringRef> {
    let (trace_id_high, trace_id_low) = split_trace_id(&link.trace_id);
    Ok(ObjectMap::from([
        (
            "trace_id".into(),
            Value::from(encode_u64_id_hex(trace_id_low)),
        ),
        (
            "trace_id_high".into(),
            Value::from(encode_u64_id_hex(trace_id_high)),
        ),
        (
            "span_id".into(),
            Value::from(encode_u64_id_hex(link.span_id)),
        ),
        (
            "attributes".into(),
            Value::from(attributes_as_strings(strings, &link.attributes)?),
        ),
        (
            "tracestate".into(),
            Value::from(strings.get(link.tracestate_ref)?),
        ),
        ("flags".into(), Value::from(i64::from(link.flags))),
    ]))
}

fn convert_idx_event(
    strings: &StringTable<'_>,
    event: &ddtrace_proto::idx::SpanEvent,
) -> Result<ObjectMap, InvalidStringRef> {
    Ok(ObjectMap::from([
        (
            "time_unix_nano".into(),
            Value::from(Utc.timestamp_nanos(u64_as_i64(event.time))),
        ),
        ("name".into(), Value::from(strings.get(event.name_ref)?)),
        (
            "attributes".into(),
            Value::from(event_attributes(strings, &event.attributes)?),
        ),
    ]))
}

struct SpanAttributes {
    meta: ObjectMap,
    metrics: ObjectMap,
    meta_struct: ObjectMap,
}

impl SpanAttributes {
    fn from_attributes(
        strings: &StringTable<'_>,
        attributes: BTreeMap<u32, AnyValue>,
    ) -> Result<Self, InvalidStringRef> {
        let mut split = Self {
            meta: ObjectMap::new(),
            metrics: ObjectMap::new(),
            meta_struct: ObjectMap::new(),
        };
        for (key_ref, value) in attributes {
            let key = strings.get(key_ref)?.to_owned();
            match value.value {
                Some(IdxValue::StringValueRef(index)) => {
                    split
                        .meta
                        .insert(key.into(), Value::from(strings.get(index)?));
                }
                Some(IdxValue::BoolValue(bool_value)) => {
                    split.meta.insert(
                        key.into(),
                        Value::from(if bool_value { "true" } else { "false" }),
                    );
                }
                Some(IdxValue::IntValue(int_value)) => {
                    split.metrics.insert(key.into(), metric_from_int(int_value));
                }
                Some(IdxValue::DoubleValue(double_value)) => {
                    split
                        .metrics
                        .insert(key.into(), metric_from_float(double_value));
                }
                Some(IdxValue::BytesValue(bytes)) => {
                    split
                        .meta_struct
                        .insert(key.into(), Value::from(Bytes::from(bytes)));
                }
                Some(value @ (IdxValue::ArrayValue(_) | IdxValue::KeyValueList(_))) => {
                    split.meta.insert(
                        key.into(),
                        Value::from(json_text(&any_value_json(
                            strings,
                            &AnyValue { value: Some(value) },
                        )?)),
                    );
                }
                None => {}
            }
        }
        Ok(split)
    }

    fn insert_promoted(&mut self, key: &str, value: &str) {
        if !value.is_empty()
            && !self.meta.contains_key(key)
            && !self.metrics.contains_key(key)
            && !self.meta_struct.contains_key(key)
        {
            self.meta.insert(key.into(), Value::from(value));
        }
    }
}

fn attributes_as_strings(
    strings: &StringTable<'_>,
    attributes: &BTreeMap<u32, AnyValue>,
) -> Result<ObjectMap, InvalidStringRef> {
    attributes
        .iter()
        .map(|(key_ref, value)| {
            Ok((
                strings.get(*key_ref)?.to_owned().into(),
                Value::from(any_value_as_string(strings, value)?),
            ))
        })
        .collect()
}

fn event_attributes(
    strings: &StringTable<'_>,
    attributes: &BTreeMap<u32, AnyValue>,
) -> Result<ObjectMap, InvalidStringRef> {
    attributes
        .iter()
        .map(|(key_ref, value)| {
            Ok((
                strings.get(*key_ref)?.to_owned().into(),
                event_attribute_value(strings, value)?,
            ))
        })
        .collect()
}

fn event_attribute_value(
    strings: &StringTable<'_>,
    value: &AnyValue,
) -> Result<Value, InvalidStringRef> {
    match &value.value {
        None => Ok(Value::Null),
        Some(IdxValue::StringValueRef(index)) => Ok(Value::from(strings.get(*index)?)),
        Some(IdxValue::BoolValue(bool_value)) => Ok(Value::from(*bool_value)),
        Some(IdxValue::IntValue(int_value)) => Ok(Value::from(*int_value)),
        Some(IdxValue::DoubleValue(double_value)) => Ok(metric_from_float(*double_value)),
        Some(IdxValue::ArrayValue(array)) if array.values.iter().all(is_scalar) => {
            Ok(Value::Array(
                array
                    .values
                    .iter()
                    .map(|item| event_attribute_value(strings, item))
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }
        Some(IdxValue::BytesValue(_) | IdxValue::ArrayValue(_) | IdxValue::KeyValueList(_)) => {
            Ok(Value::from(json_text(&any_value_json(strings, value)?)))
        }
    }
}

const fn is_scalar(value: &AnyValue) -> bool {
    matches!(
        value.value,
        None | Some(
            IdxValue::StringValueRef(_)
                | IdxValue::BoolValue(_)
                | IdxValue::IntValue(_)
                | IdxValue::DoubleValue(_)
        )
    )
}

fn any_value_as_string(
    strings: &StringTable<'_>,
    value: &AnyValue,
) -> Result<String, InvalidStringRef> {
    match &value.value {
        None => Ok(String::new()),
        Some(IdxValue::StringValueRef(index)) => Ok(strings.get(*index)?.to_owned()),
        Some(IdxValue::BoolValue(bool_value)) => {
            Ok(if *bool_value { "true" } else { "false" }.to_owned())
        }
        Some(IdxValue::IntValue(int_value)) => Ok(int_value.to_string()),
        Some(IdxValue::DoubleValue(double_value)) => Ok(format_f64(*double_value)),
        Some(IdxValue::BytesValue(bytes)) => Ok(String::from_utf8_lossy(bytes).into_owned()),
        Some(IdxValue::ArrayValue(_) | IdxValue::KeyValueList(_)) => {
            Ok(json_text(&any_value_json(strings, value)?))
        }
    }
}

fn any_value_json(
    strings: &StringTable<'_>,
    value: &AnyValue,
) -> Result<JsonValue, InvalidStringRef> {
    match &value.value {
        Some(IdxValue::StringValueRef(index)) => {
            Ok(JsonValue::String(strings.get(*index)?.to_owned()))
        }
        Some(IdxValue::BoolValue(bool_value)) => Ok(JsonValue::Bool(*bool_value)),
        Some(IdxValue::IntValue(int_value)) => Ok(JsonValue::Number((*int_value).into())),
        Some(IdxValue::DoubleValue(double_value)) => Ok(json_f64(*double_value)),
        Some(IdxValue::BytesValue(bytes)) => Ok(JsonValue::String(BASE64_STANDARD.encode(bytes))),
        Some(IdxValue::ArrayValue(array)) => Ok(JsonValue::Array(
            array
                .values
                .iter()
                .map(|item| any_value_json(strings, item))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Some(IdxValue::KeyValueList(list)) => {
            let mut entries = Vec::with_capacity(list.key_values.len());
            for entry in &list.key_values {
                let key = strings.get(entry.key)?.to_owned();
                let json = entry
                    .value
                    .as_ref()
                    .map(|value| any_value_json(strings, value))
                    .transpose()?
                    .unwrap_or(JsonValue::Null);
                entries.push((key, json));
            }
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut object = JsonMap::new();
            for (key, json) in entries {
                object.insert(key, json);
            }
            Ok(JsonValue::Object(object))
        }
        None => Ok(JsonValue::Null),
    }
}

fn json_f64(value: f64) -> JsonValue {
    serde_json::Number::from_f64(value).map_or(JsonValue::Null, JsonValue::Number)
}

fn json_text(value: &JsonValue) -> String {
    serde_json::to_string(value).expect("serializing a serde_json::Value cannot fail")
}

fn format_f64(value: f64) -> String {
    serde_json::Number::from_f64(value).map_or_else(
        || {
            if value.is_nan() {
                "NaN".to_owned()
            } else if value.is_sign_positive() {
                "+Inf".to_owned()
            } else {
                "-Inf".to_owned()
            }
        },
        |number| number.to_string(),
    )
}

fn promoted_span_kind(kind: i32) -> Option<&'static str> {
    match SpanKind::try_from(kind) {
        Ok(SpanKind::Unspecified) | Err(_) => None,
        Ok(kind) => Some(span_kind_name(kind)),
    }
}

fn metric_from_float(value: f64) -> Value {
    NotNan::new(value).map_or(Value::Null, Value::Float)
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Legacy span metrics are f64, and the Datadog traces sink drops other metric types."
)]
fn metric_from_int(value: i64) -> Value {
    metric_from_float(value as f64)
}

#[allow(
    clippy::cast_possible_wrap,
    reason = "Preserve the existing numeric conversion semantics of this source. https://github.com/vectordotdev/vector/issues/23659"
)]
const fn u64_as_i64(value: u64) -> i64 {
    value as i64
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::BTreeMap};

    use bytes::Bytes;
    use similar_asserts::assert_eq;
    use vrl::value;

    use super::{
        super::{super::ddtrace_proto, handler::convert_dd_tracer_payload},
        convert_idx_tracer_payload,
    };
    use crate::event::{ObjectMap, TraceEvent, Value};
    use ddtrace_proto::idx::{
        AnyValue, ArrayValue, KeyValue, KeyValueList, SpanKind, any_value::Value as IdxValue,
        join_trace_id,
    };

    struct Strings(RefCell<Vec<String>>);

    impl Strings {
        fn new() -> Self {
            Self(RefCell::new(vec![String::new()]))
        }

        fn from_vec(strings: Vec<String>) -> Self {
            Self(RefCell::new(strings))
        }

        fn intern(&self, value: &str) -> u32 {
            let mut strings = self.0.borrow_mut();
            strings
                .iter()
                .position(|existing| existing == value)
                .map_or_else(
                    || {
                        let index = u32::try_from(strings.len()).unwrap();
                        strings.push(value.to_owned());
                        index
                    },
                    |index| u32::try_from(index).unwrap(),
                )
        }

        fn string(&self, value: &str) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::StringValueRef(self.intern(value))),
            }
        }

        fn boolean(value: bool) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::BoolValue(value)),
            }
        }

        fn int(value: i64) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::IntValue(value)),
            }
        }

        fn double(value: f64) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::DoubleValue(value)),
            }
        }

        fn bytes(value: &[u8]) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::BytesValue(value.to_vec())),
            }
        }

        fn array(values: Vec<AnyValue>) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::ArrayValue(ArrayValue { values })),
            }
        }

        fn list(&self, entries: Vec<(&str, AnyValue)>) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::KeyValueList(KeyValueList {
                    key_values: entries
                        .into_iter()
                        .map(|(key, value)| KeyValue {
                            key: self.intern(key),
                            value: Some(value),
                        })
                        .collect(),
                })),
            }
        }

        fn map(&self, entries: Vec<(&str, AnyValue)>) -> BTreeMap<u32, AnyValue> {
            entries
                .into_iter()
                .map(|(key, value)| (self.intern(key), value))
                .collect()
        }

        fn into_payload(
            self,
            chunks: Vec<ddtrace_proto::idx::TraceChunk>,
        ) -> ddtrace_proto::idx::TracerPayload {
            ddtrace_proto::idx::TracerPayload {
                strings: self.0.into_inner(),
                chunks,
                ..Default::default()
            }
        }
    }

    fn base_span(strings: &Strings) -> ddtrace_proto::idx::Span {
        ddtrace_proto::idx::Span {
            service_ref: strings.intern("svc"),
            name_ref: strings.intern("op"),
            resource_ref: strings.intern("res"),
            type_ref: strings.intern("web"),
            span_id: 7,
            parent_id: 8,
            start: 1_000,
            duration: 50,
            ..Default::default()
        }
    }

    fn chunk(
        trace_id_high: u64,
        trace_id_low: u64,
        spans: Vec<ddtrace_proto::idx::Span>,
    ) -> ddtrace_proto::idx::TraceChunk {
        ddtrace_proto::idx::TraceChunk {
            trace_id: join_trace_id(trace_id_high, trace_id_low).to_vec(),
            spans,
            ..Default::default()
        }
    }

    fn convert(payload: ddtrace_proto::idx::TracerPayload) -> Vec<TraceEvent> {
        convert_idx_tracer_payload(payload).unwrap()
    }

    fn span_at(event: &TraceEvent, index: usize) -> &ObjectMap {
        event.as_map().get("spans").unwrap().as_array().unwrap()[index]
            .as_object()
            .unwrap()
    }

    fn object(value: &Value) -> &ObjectMap {
        value.as_object().unwrap()
    }

    #[test]
    fn span_attributes_cover_every_any_value_type() {
        let strings = Strings::new();
        let attributes = strings.map(vec![
            ("str-key", strings.string("hello")),
            ("bool-key", Strings::boolean(true)),
            ("int-key", Strings::int(2)),
            ("double-key", Strings::double(1.5)),
            ("nan-key", Strings::double(f64::NAN)),
            ("bytes-key", Strings::bytes(b"hi")),
            (
                "array-key",
                Strings::array(vec![
                    strings.string("item"),
                    Strings::boolean(true),
                    Strings::int(1),
                ]),
            ),
            (
                "map-key",
                strings.list(vec![("b", strings.string("z")), ("a", Strings::int(1))]),
            ),
        ]);
        let mut span = base_span(&strings);
        span.attributes = attributes;
        let event = &convert(strings.into_payload(vec![chunk(0, 123, vec![span])]))[0];
        let span = span_at(event, 0);

        assert_eq!(
            span.get("meta").unwrap().clone(),
            value!({
                "str-key": "hello",
                "bool-key": "true",
                "array-key": "[\"item\",true,1]",
                "map-key": "{\"a\":1,\"b\":\"z\"}",
            })
        );
        assert_eq!(
            span.get("metrics").unwrap().clone(),
            value!({
                "int-key": 2.0,
                "double-key": 1.5,
                "nan-key": null,
            })
        );
        assert_eq!(
            object(span.get("meta_struct").unwrap()),
            &ObjectMap::from([("bytes-key".into(), Value::from(Bytes::from_static(b"hi")),)])
        );
        assert!(!object(span.get("meta").unwrap()).contains_key("span.kind"));
    }

    #[test]
    fn span_events_keep_scalars_and_json_encode_compound_values() {
        let strings = Strings::new();
        let attributes = strings.map(vec![
            ("str", strings.string("hello")),
            ("flag", Strings::boolean(true)),
            ("count", Strings::int(4)),
            ("ratio", Strings::double(0.5)),
            ("missing", Strings::double(f64::NAN)),
            ("blob", Strings::bytes(b"hi")),
            (
                "scalars",
                Strings::array(vec![strings.string("x"), Strings::int(3)]),
            ),
            (
                "nested",
                Strings::array(vec![Strings::array(vec![Strings::int(1)])]),
            ),
            ("pairs", strings.list(vec![("k", Strings::boolean(true))])),
        ]);
        let mut span = base_span(&strings);
        span.events = vec![ddtrace_proto::idx::SpanEvent {
            time: 99,
            name_ref: strings.intern("exception"),
            attributes,
        }];
        let event = &convert(strings.into_payload(vec![chunk(0, 1, vec![span])]))[0];
        let attributes = object(
            object(
                &span_at(event, 0)
                    .get("span_events")
                    .unwrap()
                    .as_array()
                    .unwrap()[0],
            )
            .get("attributes")
            .unwrap(),
        );

        assert_eq!(attributes.get("str").unwrap(), &Value::from("hello"));
        assert_eq!(attributes.get("flag").unwrap(), &Value::from(true));
        assert_eq!(attributes.get("count").unwrap(), &Value::from(4));
        assert_eq!(attributes.get("blob").unwrap(), &Value::from("\"aGk=\""));
        assert_eq!(attributes.get("nested").unwrap(), &Value::from("[[1]]"));
        assert_eq!(
            attributes.get("pairs").unwrap(),
            &Value::from("{\"k\":true}")
        );
        assert_eq!(attributes.get("scalars").unwrap().clone(), value!(["x", 3]));
        assert!(matches!(attributes.get("ratio").unwrap(), Value::Float(_)));
        assert_eq!(attributes.get("missing").unwrap(), &Value::Null);
    }

    #[test]
    fn link_attributes_are_strings_and_trace_ids_are_split() {
        let strings = Strings::new();
        let attributes = strings.map(vec![
            ("link", strings.string("yes")),
            ("flag", Strings::boolean(false)),
            ("blob", Strings::bytes(b"hi")),
            ("items", Strings::array(vec![strings.string("item")])),
        ]);
        let mut span = base_span(&strings);
        span.links = vec![ddtrace_proto::idx::SpanLink {
            trace_id: vec![0x0a, 0x0b],
            span_id: 0xdead,
            attributes,
            tracestate_ref: strings.intern("vendor=1"),
            flags: 3,
        }];
        let event = &convert(strings.into_payload(vec![chunk(0, 1, vec![span])]))[0];
        let link = object(
            &span_at(event, 0)
                .get("span_links")
                .unwrap()
                .as_array()
                .unwrap()[0],
        );

        assert_eq!(
            link.get("trace_id").unwrap(),
            &Value::from("0000000000000a0b")
        );
        assert_eq!(
            link.get("trace_id_high").unwrap(),
            &Value::from("0000000000000000")
        );
        assert_eq!(
            link.get("span_id").unwrap(),
            &Value::from("000000000000dead")
        );
        assert_eq!(link.get("tracestate").unwrap(), &Value::from("vendor=1"));
        assert_eq!(link.get("flags").unwrap(), &Value::from(3));
        assert_eq!(
            link.get("attributes").unwrap().clone(),
            value!({
                "link": "yes",
                "flag": "false",
                "blob": "hi",
                "items": "[\"item\"]",
            })
        );
    }

    #[test]
    fn trace_id_high_is_written_to_the_first_span_only() {
        let strings = Strings::new();
        let first = base_span(&strings);
        let second = base_span(&strings);
        let events = convert(strings.into_payload(vec![chunk(0xab, 123, vec![first, second])]));
        let first_meta = object(span_at(&events[0], 0).get("meta").unwrap());
        let second_meta = object(span_at(&events[0], 1).get("meta").unwrap());

        assert_eq!(
            first_meta.get("_dd.p.tid").unwrap(),
            &Value::from("00000000000000ab")
        );
        assert!(!second_meta.contains_key("_dd.p.tid"));
        assert_eq!(
            span_at(&events[0], 0).get("trace_id").unwrap(),
            &Value::from(123)
        );
        assert_eq!(
            span_at(&events[0], 1).get("trace_id").unwrap(),
            &Value::from(123)
        );
    }

    #[test]
    fn trace_id_high_is_omitted_when_the_high_bits_are_zero() {
        let strings = Strings::new();
        let span = base_span(&strings);
        let events = convert(strings.into_payload(vec![chunk(0, 5, vec![span])]));
        let meta = object(span_at(&events[0], 0).get("meta").unwrap());
        assert!(!meta.contains_key("_dd.p.tid"));
    }

    #[test]
    fn existing_trace_id_high_tag_is_kept() {
        let strings = Strings::new();
        let mut span = base_span(&strings);
        span.attributes = strings.map(vec![("_dd.p.tid", strings.string("already"))]);
        let events = convert(strings.into_payload(vec![chunk(0xab, 5, vec![span])]));
        let meta = object(span_at(&events[0], 0).get("meta").unwrap());
        assert_eq!(meta.get("_dd.p.tid").unwrap(), &Value::from("already"));
    }

    #[test]
    fn sampling_mechanism_keeps_an_existing_chunk_tag() {
        let strings = Strings::new();
        let span = base_span(&strings);
        let mut trace = chunk(0, 1, vec![span]);
        trace.attributes = strings.map(vec![("_dd.p.dm", strings.string("keep"))]);
        trace.sampling_mechanism = 4;
        let events = convert(strings.into_payload(vec![trace]));
        let tags = object(events[0].as_map().get("tags").unwrap());
        assert_eq!(tags.get("_dd.p.dm").unwrap(), &Value::from("keep"));
    }

    #[test]
    fn sampling_mechanism_is_formatted_when_the_tag_is_absent() {
        let strings = Strings::new();
        let span = base_span(&strings);
        let mut trace = chunk(0, 1, vec![span]);
        trace.sampling_mechanism = 4;
        let present = convert(strings.into_payload(vec![trace]));
        assert_eq!(
            object(present[0].as_map().get("tags").unwrap())
                .get("_dd.p.dm")
                .unwrap(),
            &Value::from("-4")
        );

        let strings = Strings::new();
        let span = base_span(&strings);
        let absent = convert(strings.into_payload(vec![chunk(0, 1, vec![span])]));
        assert!(!object(absent[0].as_map().get("tags").unwrap()).contains_key("_dd.p.dm"));
    }

    #[test]
    fn payload_attribute_overrides_sampling_mechanism() {
        let strings = Strings::new();
        let span = base_span(&strings);
        let mut trace = chunk(0, 1, vec![span]);
        trace.sampling_mechanism = 4;
        let mut payload = strings.into_payload(vec![trace]);
        let strings = Strings::from_vec(payload.strings);
        payload.attributes = strings.map(vec![("_dd.p.dm", strings.string("from-payload"))]);
        payload.strings = strings.0.into_inner();
        let events = convert(payload);
        assert_eq!(
            object(events[0].as_map().get("tags").unwrap())
                .get("_dd.p.dm")
                .unwrap(),
            &Value::from("from-payload")
        );
    }

    #[test]
    fn span_kinds_are_written_unless_unspecified_or_already_set() {
        let strings = Strings::new();
        let mut spans = Vec::new();
        for kind in [
            SpanKind::Internal,
            SpanKind::Server,
            SpanKind::Client,
            SpanKind::Producer,
            SpanKind::Consumer,
            SpanKind::Unspecified,
        ] {
            let mut span = base_span(&strings);
            span.kind = i32::from(kind);
            spans.push(span);
        }
        let mut overridden = base_span(&strings);
        overridden.kind = i32::from(SpanKind::Server);
        overridden.attributes = strings.map(vec![("span.kind", strings.string("client"))]);
        overridden.env_ref = strings.intern("from-field");
        overridden
            .attributes
            .insert(strings.intern("env"), strings.string("from-attr"));
        overridden.component_ref = strings.intern("proxy");
        overridden
            .attributes
            .insert(strings.intern("component"), Strings::bytes(b"raw"));
        spans.push(overridden);

        let events = convert(strings.into_payload(vec![chunk(0, 1, spans)]));
        let kinds = ["internal", "server", "client", "producer", "consumer"];
        for (index, kind) in kinds.iter().enumerate() {
            assert_eq!(
                object(span_at(&events[0], index).get("meta").unwrap())
                    .get("span.kind")
                    .unwrap(),
                &Value::from(*kind)
            );
        }
        let unspecified = object(span_at(&events[0], 5).get("meta").unwrap());
        assert!(!unspecified.contains_key("span.kind"));

        let overridden = span_at(&events[0], 6);
        let meta = object(overridden.get("meta").unwrap());
        assert_eq!(meta.get("span.kind").unwrap(), &Value::from("client"));
        assert_eq!(meta.get("env").unwrap(), &Value::from("from-attr"));
        assert!(!meta.contains_key("component"));
        assert!(object(overridden.get("meta_struct").unwrap()).contains_key("component"));
    }

    #[test]
    fn bad_string_ref_fails() {
        let strings = Strings::new();
        let mut span = base_span(&strings);
        span.service_ref = 99;
        let error = convert_idx_tracer_payload(strings.into_payload(vec![chunk(0, 1, vec![span])]))
            .unwrap_err();
        assert_eq!(error.index, 99);
    }

    #[test]
    fn bad_link_string_ref_fails() {
        let strings = Strings::new();
        let mut span = base_span(&strings);
        span.links = vec![ddtrace_proto::idx::SpanLink {
            tracestate_ref: 99,
            ..Default::default()
        }];
        let error = convert_idx_tracer_payload(strings.into_payload(vec![chunk(0, 1, vec![span])]))
            .unwrap_err();
        assert_eq!(error.index, 99);
    }

    #[test]
    fn negative_priority_is_preserved() {
        let strings = Strings::new();
        let mut trace = chunk(0, 1, vec![base_span(&strings)]);
        trace.priority = -128;
        let events = convert(strings.into_payload(vec![trace]));
        assert_eq!(
            events[0].as_map().get("priority").unwrap(),
            &Value::from(-128_i64)
        );
    }

    #[test]
    fn idx_payload_matches_the_legacy_conversion() {
        let legacy = representative_legacy_payload();
        let idx = representative_idx_payload();
        let mut legacy_events = convert_dd_tracer_payload(legacy);
        let idx_events = convert(idx);

        let second_meta = legacy_events[0]
            .get_mut(vrl::event_path!("spans"))
            .unwrap()
            .as_array_mut()
            .unwrap()[1]
            .as_object_mut()
            .unwrap()
            .get_mut("meta")
            .unwrap()
            .as_object_mut()
            .unwrap();
        second_meta.insert("span.kind".into(), Value::from("internal"));

        assert_eq!(legacy_events, idx_events);
    }

    fn representative_legacy_payload() -> ddtrace_proto::TracerPayload {
        use ddtrace_proto::attribute_any_value::AttributeAnyValueType;

        ddtrace_proto::TracerPayload {
            container_id: "container".to_owned(),
            language_name: "rust".to_owned(),
            language_version: "1.91".to_owned(),
            tracer_version: "1.2.3".to_owned(),
            runtime_id: "runtime".to_owned(),
            app_version: "app-1".to_owned(),
            env: "legacy-env".to_owned(),
            hostname: "legacy-host".to_owned(),
            tags: BTreeMap::from([
                ("payload".to_owned(), "tag".to_owned()),
                ("shared".to_owned(), "from-payload".to_owned()),
            ]),
            chunks: vec![ddtrace_proto::TraceChunk {
                priority: 2,
                origin: "lambda".to_owned(),
                dropped_trace: true,
                tags: BTreeMap::from([
                    ("chunk".to_owned(), "tag".to_owned()),
                    ("shared".to_owned(), "from-chunk".to_owned()),
                ]),
                spans: vec![
                    ddtrace_proto::Span {
                        service: "svc".to_owned(),
                        name: "op".to_owned(),
                        resource: "res".to_owned(),
                        trace_id: 0x1111,
                        span_id: 0x2222,
                        parent_id: 0,
                        start: 1_431_648_000_000_001,
                        duration: 50,
                        error: 1,
                        meta: BTreeMap::from([
                            ("foo".to_owned(), "bar".to_owned()),
                            ("env".to_owned(), "prod".to_owned()),
                            ("version".to_owned(), "v9".to_owned()),
                            ("component".to_owned(), "proxy".to_owned()),
                            ("span.kind".to_owned(), "server".to_owned()),
                        ]),
                        metrics: BTreeMap::from([("n".to_owned(), 1.5)]),
                        r#type: "web".to_owned(),
                        meta_struct: BTreeMap::from([("bin".to_owned(), b"hi".to_vec())]),
                        span_links: vec![ddtrace_proto::SpanLink {
                            trace_id: 7,
                            trace_id_high: 8,
                            span_id: 9,
                            attributes: BTreeMap::from([("link".to_owned(), "yes".to_owned())]),
                            tracestate: "vendor=1".to_owned(),
                            flags: 1,
                        }],
                        span_events: vec![ddtrace_proto::SpanEvent {
                            time_unix_nano: 99,
                            name: "exception".to_owned(),
                            attributes: BTreeMap::from([(
                                "exception.message".to_owned(),
                                ddtrace_proto::AttributeAnyValue {
                                    r#type: i32::from(AttributeAnyValueType::StringValue),
                                    string_value: "boom".to_owned(),
                                    ..Default::default()
                                },
                            )]),
                        }],
                    },
                    ddtrace_proto::Span {
                        service: "svc2".to_owned(),
                        name: "op2".to_owned(),
                        resource: "res2".to_owned(),
                        trace_id: 0x1111,
                        span_id: 0x3333,
                        parent_id: 0x2222,
                        start: 10,
                        duration: 1,
                        meta: BTreeMap::from([("only".to_owned(), "second".to_owned())]),
                        r#type: "db".to_owned(),
                        ..Default::default()
                    },
                ],
            }],
            ..Default::default()
        }
    }

    fn representative_idx_payload() -> ddtrace_proto::idx::TracerPayload {
        let strings = Strings::new();
        let mut root = base_span(&strings);
        root.service_ref = strings.intern("svc");
        root.name_ref = strings.intern("op");
        root.resource_ref = strings.intern("res");
        root.type_ref = strings.intern("web");
        root.span_id = 0x2222;
        root.parent_id = 0;
        root.start = 1_431_648_000_000_001;
        root.duration = 50;
        root.error = true;
        root.env_ref = strings.intern("prod");
        root.version_ref = strings.intern("v9");
        root.component_ref = strings.intern("proxy");
        root.kind = i32::from(SpanKind::Server);
        root.attributes = strings.map(vec![
            ("foo", strings.string("bar")),
            ("n", Strings::double(1.5)),
            ("bin", Strings::bytes(b"hi")),
        ]);
        root.links = vec![ddtrace_proto::idx::SpanLink {
            trace_id: join_trace_id(8, 7).to_vec(),
            span_id: 9,
            attributes: strings.map(vec![("link", strings.string("yes"))]),
            tracestate_ref: strings.intern("vendor=1"),
            flags: 1,
        }];
        root.events = vec![ddtrace_proto::idx::SpanEvent {
            time: 99,
            name_ref: strings.intern("exception"),
            attributes: strings.map(vec![("exception.message", strings.string("boom"))]),
        }];

        let mut child = base_span(&strings);
        child.service_ref = strings.intern("svc2");
        child.name_ref = strings.intern("op2");
        child.resource_ref = strings.intern("res2");
        child.type_ref = strings.intern("db");
        child.span_id = 0x3333;
        child.parent_id = 0x2222;
        child.start = 10;
        child.duration = 1;
        child.kind = i32::from(SpanKind::Internal);
        child.attributes = strings.map(vec![("only", strings.string("second"))]);

        let mut trace = chunk(0, 0x1111, vec![root, child]);
        trace.priority = 2;
        trace.origin_ref = strings.intern("lambda");
        trace.dropped_trace = true;
        trace.attributes = strings.map(vec![
            ("chunk", strings.string("tag")),
            ("shared", strings.string("from-chunk")),
        ]);

        let mut payload = strings.into_payload(vec![trace]);
        let strings = Strings::from_vec(payload.strings);
        payload.container_id_ref = strings.intern("container");
        payload.language_name_ref = strings.intern("rust");
        payload.language_version_ref = strings.intern("1.91");
        payload.tracer_version_ref = strings.intern("1.2.3");
        payload.runtime_id_ref = strings.intern("runtime");
        payload.app_version_ref = strings.intern("app-1");
        payload.env_ref = strings.intern("idx-env");
        payload.hostname_ref = strings.intern("idx-host");
        payload.attributes = strings.map(vec![
            ("payload", strings.string("tag")),
            ("shared", strings.string("from-payload")),
        ]);
        payload.container_debug = Some(ddtrace_proto::idx::ContainerDebug {
            was_buffered: true,
            ..Default::default()
        });
        payload.strings = strings.0.into_inner();
        payload
    }
}
