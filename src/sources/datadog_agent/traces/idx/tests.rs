use std::{cell::RefCell, collections::BTreeMap};

use bytes::Bytes;
use proptest::{
    num::f64::{NEGATIVE, NORMAL, POSITIVE, SUBNORMAL, ZERO},
    prelude::*,
};
use similar_asserts::assert_eq;
use vrl::value;

use super::{
    super::{super::ddtrace_proto, handler::convert_dd_tracer_payload},
    ExpansionBudget, IdxConversionError, convert_idx_tracer_payload, format_f64, json_string_len,
    lossy_utf8_len,
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
    convert_idx_tracer_payload(payload, &ExpansionBudget::new(usize::MAX)).unwrap()
}

fn convert_err(payload: ddtrace_proto::idx::TracerPayload) -> IdxConversionError {
    convert_idx_tracer_payload(payload, &ExpansionBudget::new(usize::MAX)).unwrap_err()
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
fn trace_id_high_is_written_to_the_logical_root_only() {
    let strings = Strings::new();
    let mut child = base_span(&strings);
    child.span_id = 2;
    child.parent_id = 1;
    let mut root = base_span(&strings);
    root.span_id = 1;
    root.parent_id = 0;
    let events = convert(strings.into_payload(vec![chunk(0xab, 123, vec![child, root])]));
    let child_meta = object(span_at(&events[0], 0).get("meta").unwrap());
    let root_meta = object(span_at(&events[0], 1).get("meta").unwrap());

    assert_eq!(
        root_meta.get("_dd.p.tid").unwrap(),
        &Value::from("00000000000000ab")
    );
    assert!(!child_meta.contains_key("_dd.p.tid"));
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
fn trace_id_high_falls_back_to_the_first_span_when_no_span_is_a_root() {
    let strings = Strings::new();
    let mut first = base_span(&strings);
    first.span_id = 1;
    first.parent_id = 9;
    let mut second = base_span(&strings);
    second.span_id = 2;
    second.parent_id = 9;
    let events = convert(strings.into_payload(vec![chunk(0xab, 123, vec![first, second])]));
    let first_meta = object(span_at(&events[0], 0).get("meta").unwrap());
    let second_meta = object(span_at(&events[0], 1).get("meta").unwrap());

    assert_eq!(
        first_meta.get("_dd.p.tid").unwrap(),
        &Value::from("00000000000000ab")
    );
    assert!(!second_meta.contains_key("_dd.p.tid"));
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
fn chunk_attribute_doubles_match_the_agent_as_string() {
    // Spellings taken from Go's strconv.FormatFloat(v, 'f', -1, 64), which
    // AnyValue.AsString uses. Integral values have no decimal point, and
    // large values stay in fixed point rather than scientific notation.
    let cases = [
        ("hundred", 100.0, "100"),
        ("tiny", 1e-7, "0.0000001"),
        ("huge", 1e21, "1000000000000000000000"),
        ("neg-zero", -0.0, "-0"),
        ("pos-inf", f64::INFINITY, "+Inf"),
        ("neg-inf", f64::NEG_INFINITY, "-Inf"),
        ("nan", f64::NAN, "NaN"),
        ("negative", -1.5, "-1.5"),
        // f64::from_bits(0x42d85e4863b34e08) is exactly 107172881943864.125,
        // halfway between its two shortest round-trip spellings. Go's Ryu port
        // breaks the tie to even (.12); Rust's `Display` breaks it the other way
        // (.13), so this pins the Go spelling independent of `to_string`.
        (
            "tie",
            f64::from_bits(0x42d8_5e48_63b3_4e08),
            "107172881943864.12",
        ),
    ];
    let strings = Strings::new();
    let attributes = strings.map(
        cases
            .iter()
            .map(|(key, value, _)| (*key, Strings::double(*value)))
            .collect(),
    );
    let mut trace = chunk(0, 1, vec![base_span(&strings)]);
    trace.attributes = attributes;
    let events = convert(strings.into_payload(vec![trace]));
    let tags = object(events[0].as_map().get("tags").unwrap());
    for (key, _, expected) in cases {
        assert_eq!(tags.get(key).unwrap(), &Value::from(expected));
    }
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
}

#[test]
fn sampling_mechanism_zero_adds_no_tag() {
    let strings = Strings::new();
    let span = base_span(&strings);
    let absent = convert(strings.into_payload(vec![chunk(0, 1, vec![span])]));
    assert!(!object(absent[0].as_map().get("tags").unwrap()).contains_key("_dd.p.dm"));
}

#[test]
fn sampling_mechanism_zero_keeps_an_explicit_chunk_tag() {
    let strings = Strings::new();
    let span = base_span(&strings);
    let mut trace = chunk(0, 1, vec![span]);
    trace.attributes = strings.map(vec![("_dd.p.dm", strings.string("-0"))]);
    let events = convert(strings.into_payload(vec![trace]));
    assert_eq!(
        object(events[0].as_map().get("tags").unwrap())
            .get("_dd.p.dm")
            .unwrap(),
        &Value::from("-0")
    );
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
    let error = convert_err(strings.into_payload(vec![chunk(0, 1, vec![span])]));
    assert!(matches!(
        error,
        IdxConversionError::InvalidStringRef(ref error) if error.index == 99
    ));
}

#[test]
fn bad_link_string_ref_fails() {
    let strings = Strings::new();
    let mut span = base_span(&strings);
    span.links = vec![ddtrace_proto::idx::SpanLink {
        tracestate_ref: 99,
        ..Default::default()
    }];
    let error = convert_err(strings.into_payload(vec![chunk(0, 1, vec![span])]));
    assert!(matches!(
        error,
        IdxConversionError::InvalidStringRef(ref error) if error.index == 99
    ));
}

fn resource_fan_out_payload(
    resource_len: usize,
    spans: usize,
) -> ddtrace_proto::idx::TracerPayload {
    let strings = Strings::new();
    let resource_ref = strings.intern(&"r".repeat(resource_len));
    let spans = (0..spans)
        .map(|_| {
            let mut span = base_span(&strings);
            span.resource_ref = resource_ref;
            span
        })
        .collect();
    strings.into_payload(vec![chunk(0, 1, spans)])
}

#[test]
fn empty_string_refs_still_count_against_the_budget() {
    let strings = Strings::new();
    let spans = (0..1024)
        .map(|_| ddtrace_proto::idx::Span::default())
        .collect();
    let error = convert_idx_tracer_payload(
        strings.into_payload(vec![chunk(0, 1, spans)]),
        &ExpansionBudget::new(4096),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        IdxConversionError::ExpansionLimitExceeded { .. }
    ));
}

/// `strconv.FormatFloat(5e-324, 'f', -1, 64)`: `"0."`, 323 zeros, then `"5"`.
fn min_subnormal_fixed_point() -> String {
    let mut text = String::from("0.");
    text.extend(std::iter::repeat_n('0', 323));
    text.push('5');
    text
}

fn subnormal_chunk_attributes(count: usize) -> ddtrace_proto::idx::TracerPayload {
    let strings = Strings::new();
    let keys: Vec<String> = (0..count).map(|index| format!("k{index}")).collect();
    let mut trace = chunk(0, 1, Vec::new());
    trace.attributes = strings.map(
        keys.iter()
            .map(|key| (key.as_str(), Strings::double(5e-324)))
            .collect(),
    );
    strings.into_payload(vec![trace])
}

#[test]
fn subnormal_chunk_attribute_doubles_count_against_the_expansion_budget() {
    let spelling = min_subnormal_fixed_point();
    assert_eq!(spelling.len(), 326);

    let error =
        convert_idx_tracer_payload(subnormal_chunk_attributes(30), &ExpansionBudget::new(4096))
            .unwrap_err();
    assert!(matches!(
        error,
        IdxConversionError::ExpansionLimitExceeded { limit: 4096 }
    ));

    let events = convert_idx_tracer_payload(
        subnormal_chunk_attributes(30),
        &ExpansionBudget::new(32 * 1024),
    )
    .unwrap();
    let tags = object(events[0].as_map().get("tags").unwrap());
    assert_eq!(tags.len(), 30);
    assert_eq!(tags.get("k0").unwrap(), &Value::from(spelling));
}

#[test]
fn integer_chunk_attributes_count_their_tag_strings_against_the_budget() {
    // Each value is `-9223372036854775808`, which is 20 bytes as a tag string.
    let attributes = |strings: &Strings| {
        let keys: Vec<String> = (0..64).map(|index| format!("k{index}")).collect();
        strings.map(
            keys.iter()
                .map(|key| (key.as_str(), Strings::int(i64::MIN)))
                .collect(),
        )
    };

    let strings = Strings::new();
    let mut trace = chunk(0, 1, Vec::new());
    trace.attributes = attributes(&strings);
    let error = convert_idx_tracer_payload(
        strings.into_payload(vec![trace]),
        &ExpansionBudget::new(4096),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        IdxConversionError::ExpansionLimitExceeded { limit: 4096 }
    ));

    let strings = Strings::new();
    let mut trace = chunk(0, 1, Vec::new());
    trace.attributes = attributes(&strings);
    let events = convert_idx_tracer_payload(
        strings.into_payload(vec![trace]),
        &ExpansionBudget::new(64 * 1024),
    )
    .unwrap();
    let tags = object(events[0].as_map().get("tags").unwrap());
    assert_eq!(
        tags.get("k0").unwrap(),
        &Value::from("-9223372036854775808")
    );
}

#[test]
fn scalar_array_elements_count_against_the_expansion_budget() {
    // An unset `AnyValue` encodes in two bytes but becomes a `Value::Null` element.
    let nulls = |strings: &Strings| {
        let mut span = base_span(strings);
        span.events = vec![ddtrace_proto::idx::SpanEvent {
            attributes: strings.map(vec![(
                "k",
                Strings::array(vec![AnyValue { value: None }; 1024]),
            )]),
            ..Default::default()
        }];
        chunk(0, 1, vec![span])
    };

    let strings = Strings::new();
    let trace = nulls(&strings);
    let error = convert_idx_tracer_payload(
        strings.into_payload(vec![trace]),
        &ExpansionBudget::new(4096),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        IdxConversionError::ExpansionLimitExceeded { limit: 4096 }
    ));

    let strings = Strings::new();
    let trace = nulls(&strings);
    let events = convert_idx_tracer_payload(
        strings.into_payload(vec![trace]),
        &ExpansionBudget::new(256 * 1024),
    )
    .unwrap();
    let span_events = span_at(&events[0], 0)["span_events"].as_array().unwrap();
    let attributes = object(&object(&span_events[0])["attributes"]);
    assert_eq!(attributes["k"].as_array().unwrap().len(), 1024);
}

#[test]
fn link_ids_count_against_the_expansion_budget() {
    // An unset link encodes in two bytes but emits three 16-character hex ids, which
    // count against the budget.
    let links = |strings: &Strings| {
        let mut span = base_span(strings);
        span.links = vec![ddtrace_proto::idx::SpanLink::default(); 1024];
        chunk(0, 1, vec![span])
    };

    let strings = Strings::new();
    let trace = links(&strings);
    let error = convert_idx_tracer_payload(
        strings.into_payload(vec![trace]),
        &ExpansionBudget::new(1024 * 64),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        IdxConversionError::ExpansionLimitExceeded { .. }
    ));

    let strings = Strings::new();
    let trace = links(&strings);
    let events = convert_idx_tracer_payload(
        strings.into_payload(vec![trace]),
        &ExpansionBudget::new(1024 * 256),
    )
    .unwrap();
    assert_eq!(
        span_at(&events[0], 0)["span_links"]
            .as_array()
            .unwrap()
            .len(),
        1024
    );
}

#[test]
fn repeated_string_refs_exceed_the_expansion_budget() {
    let payload = resource_fan_out_payload(1024, 64);
    let error = convert_idx_tracer_payload(payload, &ExpansionBudget::new(32 * 1024)).unwrap_err();
    assert!(matches!(
        error,
        IdxConversionError::ExpansionLimitExceeded { limit: 32_768 }
    ));

    let payload = resource_fan_out_payload(1024, 64);
    let events = convert_idx_tracer_payload(payload, &ExpansionBudget::new(128 * 1024)).unwrap();
    assert_eq!(events[0].as_map()["spans"].as_array().unwrap().len(), 64);
}

#[test]
fn payload_tags_copied_into_each_chunk_count_against_the_budget() {
    let strings = Strings::new();
    let payload_tag = strings.string(&"t".repeat(1024));
    let chunks = (1..=64).map(|low| chunk(0, low, Vec::new())).collect();
    let mut payload = strings.into_payload(chunks);
    let tag_key = u32::try_from(payload.strings.len()).unwrap();
    payload.strings.push("payload-tag".to_owned());
    payload.attributes = BTreeMap::from([(tag_key, payload_tag)]);

    let error = convert_idx_tracer_payload(payload, &ExpansionBudget::new(32 * 1024)).unwrap_err();
    assert!(matches!(
        error,
        IdxConversionError::ExpansionLimitExceeded { .. }
    ));
}

proptest! {
    #[test]
    fn json_string_len_matches_serde_json(value: String) {
        prop_assert_eq!(
            json_string_len(&value),
            serde_json::to_string(&value).unwrap().len()
        );
    }

    #[test]
    fn lossy_utf8_len_matches_from_utf8_lossy(bytes: Vec<u8>) {
        prop_assert_eq!(
            lossy_utf8_len(&bytes),
            String::from_utf8_lossy(&bytes).len()
        );
    }

    /// Parsing the output must recover the input bits, so digit placement cannot
    /// drift. The literal cases in `chunk_attribute_doubles_match_the_agent_as_string`
    /// pin Go's tie-breaking, which `f64::parse` cannot observe on its own.
    #[test]
    fn format_f64_round_trips(value in POSITIVE | NEGATIVE | NORMAL | SUBNORMAL | ZERO) {
        let formatted = format_f64(value);
        prop_assert!(!formatted.contains('e'));
        prop_assert_eq!(formatted.parse::<f64>().unwrap().to_bits(), value.to_bits());
    }
}

#[test]
fn lossy_utf8_replacement_counts_against_the_budget() {
    let strings = Strings::new();
    let mut trace = chunk(0, 1, Vec::new());
    trace.attributes = strings.map(vec![("k", Strings::bytes(&[0xff; 1024]))]);
    let error = convert_idx_tracer_payload(
        strings.into_payload(vec![trace]),
        &ExpansionBudget::new(2048),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        IdxConversionError::ExpansionLimitExceeded { .. }
    ));
}

#[test]
fn json_escaping_counts_against_the_budget() {
    let strings = Strings::new();
    let mut span = base_span(&strings);
    span.attributes = strings.map(vec![(
        "k",
        Strings::array(vec![strings.string(&"\u{1}".repeat(1024))]),
    )]);
    let error = convert_idx_tracer_payload(
        strings.into_payload(vec![chunk(0, 1, vec![span])]),
        &ExpansionBudget::new(4096),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        IdxConversionError::ExpansionLimitExceeded { .. }
    ));
}

#[test]
fn expansion_budget_is_shared_across_payloads() {
    let budget = ExpansionBudget::new(96 * 1024);
    convert_idx_tracer_payload(resource_fan_out_payload(1024, 64), &budget).unwrap();
    let error =
        convert_idx_tracer_payload(resource_fan_out_payload(1024, 64), &budget).unwrap_err();
    assert!(matches!(
        error,
        IdxConversionError::ExpansionLimitExceeded { .. }
    ));
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
    let root = ddtrace_proto::idx::Span {
        service_ref: strings.intern("svc"),
        name_ref: strings.intern("op"),
        resource_ref: strings.intern("res"),
        type_ref: strings.intern("web"),
        span_id: 0x2222,
        parent_id: 0,
        start: 1_431_648_000_000_001,
        duration: 50,
        error: true,
        env_ref: strings.intern("prod"),
        version_ref: strings.intern("v9"),
        component_ref: strings.intern("proxy"),
        kind: i32::from(SpanKind::Server),
        attributes: strings.map(vec![
            ("foo", strings.string("bar")),
            ("n", Strings::double(1.5)),
            ("bin", Strings::bytes(b"hi")),
        ]),
        links: vec![ddtrace_proto::idx::SpanLink {
            trace_id: join_trace_id(8, 7).to_vec(),
            span_id: 9,
            attributes: strings.map(vec![("link", strings.string("yes"))]),
            tracestate_ref: strings.intern("vendor=1"),
            flags: 1,
        }],
        events: vec![ddtrace_proto::idx::SpanEvent {
            time: 99,
            name_ref: strings.intern("exception"),
            attributes: strings.map(vec![("exception.message", strings.string("boom"))]),
        }],
    };

    let child = ddtrace_proto::idx::Span {
        service_ref: strings.intern("svc2"),
        name_ref: strings.intern("op2"),
        resource_ref: strings.intern("res2"),
        type_ref: strings.intern("db"),
        span_id: 0x3333,
        parent_id: 0x2222,
        start: 10,
        duration: 1,
        kind: i32::from(SpanKind::Internal),
        attributes: strings.map(vec![("only", strings.string("second"))]),
        ..base_span(&strings)
    };

    let trace = ddtrace_proto::idx::TraceChunk {
        priority: 2,
        origin_ref: strings.intern("lambda"),
        dropped_trace: true,
        attributes: strings.map(vec![
            ("chunk", strings.string("tag")),
            ("shared", strings.string("from-chunk")),
        ]),
        ..chunk(0, 0x1111, vec![root, child])
    };

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
