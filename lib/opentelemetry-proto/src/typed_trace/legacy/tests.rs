use std::time::Duration;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use similar_asserts::assert_eq;
use vector_core::event::{
    EventMetadata, TraceEvent as LegacyTraceEvent, TraceLayout,
    typed_trace::{
        AttrMap, AttrValue, Attributes, DroppedCount, OtlpSpanFlags, PanicOnIssue, Scope, Span,
        SpanEvent, SpanId, SpanKind, SpanLink, SpanStatus, TraceConversionCounts, TraceEvent,
        TraceId,
    },
};
use vrl::value::{ObjectMap, Value};

use super::{
    ConversionError, FailedConversion, HintedLegacy, hinted_legacy_to_typed,
    is_historical_otlp_layout, legacy_to_typed,
};
use crate::{
    proto::{
        common::v1::{
            AnyValue, ArrayValue, InstrumentationScope, KeyValue, any_value::Value as PbValue,
        },
        resource::v1::Resource as PbResource,
        trace::v1::{
            ResourceSpans, ScopeSpans, Span as PbSpan, Status as PbStatus,
            span::{Event as PbEvent, Link as PbLink},
        },
    },
    spans,
    typed_trace::test_support::{attrs, rejected, sid, tid, typed_event_with_metadata, typed_link},
};

fn convert_legacy(legacy: LegacyTraceEvent) -> (Vec<TraceEvent>, TraceConversionCounts) {
    let mut counts = TraceConversionCounts::default();
    let events = legacy_to_typed(legacy, &mut counts).unwrap();
    (events, counts)
}

fn convert_hinted(legacy: LegacyTraceEvent) -> (Vec<TraceEvent>, TraceConversionCounts) {
    let mut counts = TraceConversionCounts::default();
    let HintedLegacy::Done(events) = hinted_legacy_to_typed(legacy, &mut counts) else {
        panic!("an OTLP layout hint");
    };
    (events.unwrap(), counts)
}

fn traces(events: Vec<TraceEvent>) -> (Vec<TraceEvent>, TraceConversionCounts) {
    (events, TraceConversionCounts::default())
}

/// The single event a one-span legacy record converts to, carrying the record's metadata.
fn legacy_span(legacy: &LegacyTraceEvent, span: Span) -> Vec<TraceEvent> {
    vec![typed_event_with_metadata(1, vec![span], legacy.metadata())]
}

/// A flattened per-span record with its layout hint, as the `opentelemetry` source writes it.
fn flattened_legacy() -> LegacyTraceEvent {
    let mut event = LegacyTraceEvent::default();
    event
        .metadata_mut()
        .set_trace_layout(TraceLayout::OtelFlattened);
    event.insert(
        vrl::event_path!(spans::TRACE_ID_KEY),
        Value::from("00000000000000000000000000000001"),
    );
    event.insert(
        vrl::event_path!(spans::SPAN_ID_KEY),
        Value::from("0000000000000001"),
    );
    event.insert(vrl::event_path!("name"), Value::from("s"));
    event.insert(
        vrl::event_path!("start_time_unix_nano"),
        Value::from(DateTime::<Utc>::UNIX_EPOCH),
    );
    event
}

/// A `resourceSpans` record with its layout hint, as the `opentelemetry` source writes it with
/// `use_otlp_decoding`.
fn envelope(resource_spans: Vec<Value>) -> LegacyTraceEvent {
    let mut envelope = LegacyTraceEvent::default();
    envelope
        .metadata_mut()
        .set_trace_layout(TraceLayout::OtlpResourceSpans);
    envelope.insert(
        vrl::event_path!("resourceSpans"),
        Value::Array(resource_spans),
    );
    envelope
}

fn resource_spans_value(scope_spans: Vec<Value>) -> Value {
    Value::Object(ObjectMap::from([(
        "scopeSpans".into(),
        Value::Array(scope_spans),
    )]))
}

fn scope_spans_value(spans: Vec<Value>) -> Value {
    Value::Object(ObjectMap::from([("spans".into(), Value::Array(spans))]))
}

/// An envelope holding one resource and one scope grouping of `spans`.
fn envelope_with_spans(spans: Vec<Value>) -> LegacyTraceEvent {
    envelope(vec![resource_spans_value(vec![scope_spans_value(spans)])])
}

fn envelope_span(trace_id: Value, span_id: Value) -> ObjectMap {
    ObjectMap::from([
        ("traceId".into(), trace_id),
        ("spanId".into(), span_id),
        ("name".into(), Value::from("s")),
    ])
}

fn valid_envelope_span(trace: u128, span: u64) -> ObjectMap {
    envelope_span(raw_trace_id(trace), raw_span_id(span))
}

/// A trace ID as the protobuf decoder stores it: raw big-endian bytes.
fn raw_trace_id(trace: u128) -> Value {
    Value::Bytes(Bytes::copy_from_slice(&tid(trace).to_bytes()))
}

/// A span ID as the protobuf decoder stores it: raw big-endian bytes.
fn raw_span_id(span: u64) -> Value {
    Value::Bytes(Bytes::copy_from_slice(&sid(span).to_bytes()))
}

#[test]
fn detection_matches_hinted_conversion_and_rejects_datadog() {
    for legacy in [
        flattened_legacy(),
        envelope_with_spans(vec![Value::Object(valid_envelope_span(1, 1))]),
    ] {
        assert!(is_historical_otlp_layout(&legacy));
        let mut hintless = legacy.clone();
        hintless.metadata_mut().clear_trace_layout();
        let (mut detected, counts) = convert_legacy(hintless.clone());
        for event in &mut detected {
            *event.metadata_mut() = legacy.metadata().clone();
        }
        assert_eq!((detected, counts), convert_hinted(legacy.clone()));
    }

    let mut datadog = LegacyTraceEvent::default();
    datadog.insert(vrl::event_path!("spans"), Value::Array(Vec::new()));
    datadog.insert(vrl::event_path!("priority"), Value::Integer(1));
    assert!(!is_historical_otlp_layout(&datadog));
    assert_eq!(
        legacy_to_typed(datadog.clone(), &mut PanicOnIssue),
        Err(FailedConversion(datadog, ConversionError::NotOtlpLayout))
    );
}

#[test]
fn flattened_absent_end_uses_protobuf_zero_and_reports_when_reversed() {
    let mut span = flattened_legacy();
    let start = DateTime::from_timestamp(1, 0).unwrap();
    span.insert(vrl::event_path!("start_time_unix_nano"), Value::from(start));

    assert_eq!(
        convert_hinted(span.clone()),
        (
            legacy_span(
                &span,
                Span {
                    start_time: start,
                    ..Span::new(sid(1), "s")
                },
            ),
            TraceConversionCounts {
                reversed_span_timestamps: 1,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn flattened_legacy_conversion_uses_typed_defaults() {
    let mut flattened = flattened_legacy();
    flattened.insert(vrl::event_path!("kind"), Value::Integer(1));
    flattened.insert(
        vrl::event_path!("end_time_unix_nano"),
        Value::from(DateTime::<Utc>::UNIX_EPOCH),
    );
    assert_eq!(
        convert_hinted(flattened.clone()),
        traces(legacy_span(
            &flattened,
            Span {
                kind: SpanKind::Internal,
                ..Span::new(sid(1), "s")
            },
        ))
    );
}

#[test]
fn envelope_layout_hint_converts_resource_spans() {
    let mut span = valid_envelope_span(1, 1);
    span.insert("startTimeUnixNano".into(), Value::Integer(10));
    span.insert("endTimeUnixNano".into(), Value::Integer(20));
    let envelope = envelope_with_spans(vec![Value::Object(span)]);
    assert_eq!(
        convert_hinted(envelope.clone()),
        traces(legacy_span(
            &envelope,
            Span {
                start_time: DateTime::from_timestamp_nanos(10),
                duration: Duration::from_nanos(10),
                ..Span::new(sid(1), "s")
            },
        ))
    );
}

/// A two-span batch covering the fields the flattened producer keeps.
fn producer_batch() -> ResourceSpans {
    let child = PbSpan {
        parent_span_id: sid(1).to_bytes().to_vec(),
        trace_state: "k=v".into(),
        flags: 0x301,
        kind: SpanKind::Server.to_i32(),
        attributes: vec![
            KeyValue::from_string("http.route", "/a"),
            KeyValue::from_int("count", 3),
            KeyValue::from_parts("ok", Some(AnyValue::from_bool(true))),
            KeyValue::from_parts("ratio", Some(AnyValue::from_double(0.5))),
            KeyValue::from_parts("raw", Some(AnyValue::from_bytes(vec![0xff, 0xfe]))),
            KeyValue::from_parts(
                "list",
                Some(AnyValue {
                    value: Some(PbValue::ArrayValue(ArrayValue {
                        values: vec![AnyValue::from_int(1), AnyValue::from_string("x")],
                    })),
                }),
            ),
            KeyValue::from_parts(
                "nested",
                Some(AnyValue::from_kvlist(vec![KeyValue::from_string("k", "v")])),
            ),
            KeyValue::from_string("datadog.span.resource", "GET /a"),
        ],
        dropped_attributes_count: 1,
        events: vec![PbEvent {
            time_unix_nano: 1_500,
            name: "e".into(),
            attributes: vec![KeyValue::from_int("n", 1)],
            dropped_attributes_count: 2,
        }],
        dropped_events_count: 3,
        links: vec![PbLink {
            trace_state: "l=1".into(),
            flags: 0x101,
            attributes: vec![KeyValue::from_string("lk", "lv")],
            dropped_attributes_count: 4,
            ..PbLink::from_parts(9, 9)
        }],
        dropped_links_count: 5,
        status: Some(PbStatus {
            code: SpanStatus::ERROR,
            message: "boom".into(),
        }),
        ..PbSpan::from_parts(2, 2, "child")
    };
    ResourceSpans {
        resource: Some(PbResource {
            attributes: vec![
                KeyValue::from_string("service.name", "svc"),
                KeyValue::from_string("region", "us"),
                KeyValue::from_parts(
                    "datadog.tracer.tags",
                    Some(AnyValue::from_kvlist(vec![KeyValue::from_string("t", "1")])),
                ),
            ],
            dropped_attributes_count: 6,
        }),
        scope_spans: vec![ScopeSpans {
            scope: Some(InstrumentationScope {
                name: "lib".into(),
                version: "1".into(),
                attributes: vec![KeyValue::from_string("s", "v")],
                dropped_attributes_count: 7,
            }),
            spans: vec![PbSpan::from_parts(1, 1, "root"), child],
            schema_url: "https://scope".into(),
        }],
        schema_url: "https://resource".into(),
    }
}

#[test]
fn flattened_producer_and_converter_agree() {
    let rs = producer_batch();

    let mut metadata = EventMetadata::default();
    metadata.set_trace_layout(TraceLayout::OtelFlattened);
    let expected: Vec<TraceEvent> = rs
        .clone()
        .into_typed_events(&metadata, &mut PanicOnIssue)
        .into_iter()
        .map(|mut event| {
            // Fields the flattened per-span layout does not carry.
            event.resource_mut().schema_url = None;
            event.resource_mut().dropped_attributes_count = DroppedCount::ZERO;
            *event.scope_mut() = Scope::default();
            for span in event.spans_mut() {
                span.flags = OtlpSpanFlags::none();
                for link in &mut span.links {
                    link.flags = OtlpSpanFlags::none();
                }
            }
            event
        })
        .collect();
    let actual: Vec<TraceEvent> = rs
        .into_event_iter()
        .flat_map(|event| {
            let HintedLegacy::Done(events) =
                hinted_legacy_to_typed(event.into_trace(), &mut PanicOnIssue)
            else {
                panic!("the source sets an OTLP layout hint");
            };
            events.expect("OTLP layout conversion")
        })
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn flattened_hint_skips_detection_so_empty_trace_id_rejects_span() {
    let mut span = flattened_legacy();
    span.insert(vrl::event_path!(spans::TRACE_ID_KEY), Value::from(""));
    assert_eq!(convert_hinted(span.clone()), rejected(1));
    assert_eq!(convert_legacy(span.clone()), rejected(1));

    span.metadata_mut().clear_trace_layout();
    assert_eq!(
        legacy_to_typed(span.clone(), &mut PanicOnIssue),
        Err(FailedConversion(span, ConversionError::NotOtlpLayout))
    );
}

#[test]
fn envelope_hint_requires_resource_spans_array() {
    let mut event = flattened_legacy();
    event
        .metadata_mut()
        .set_trace_layout(TraceLayout::OtlpResourceSpans);
    assert_eq!(
        hinted_legacy_to_typed(event.clone(), &mut PanicOnIssue),
        HintedLegacy::Done(Err(FailedConversion(
            event.clone(),
            ConversionError::NotOtlpLayout
        )))
    );

    event.insert(vrl::event_path!("resourceSpans"), Value::from("nope"));
    assert_eq!(
        hinted_legacy_to_typed(event.clone(), &mut PanicOnIssue),
        HintedLegacy::Done(Err(FailedConversion(event, ConversionError::NotOtlpLayout)))
    );
}

#[test]
fn absent_and_non_otlp_hints_are_not_converted() {
    let mut span = flattened_legacy();
    span.metadata_mut().clear_trace_layout();
    assert_eq!(
        hinted_legacy_to_typed(span.clone(), &mut PanicOnIssue),
        HintedLegacy::Skipped(span.clone())
    );

    span.metadata_mut().set_trace_layout(TraceLayout::Datadog);
    assert_eq!(
        hinted_legacy_to_typed(span.clone(), &mut PanicOnIssue),
        HintedLegacy::Skipped(span.clone())
    );
    // The record has a flattened OTLP shape, but the hint names another producer.
    assert_eq!(
        legacy_to_typed(span.clone(), &mut PanicOnIssue),
        Err(FailedConversion(span, ConversionError::NotOtlpLayout))
    );
}

#[test]
fn unrecognized_hint_is_an_error_without_shape_detection() {
    let mut span = flattened_legacy();
    span.metadata_mut()
        .set_trace_layout(TraceLayout::Unrecognized(99));
    let failed = FailedConversion(
        span.clone(),
        ConversionError::UnrecognizedLayout { layout: 99 },
    );
    assert_eq!(
        hinted_legacy_to_typed(span.clone(), &mut PanicOnIssue),
        HintedLegacy::Done(Err(failed.clone()))
    );
    assert_eq!(legacy_to_typed(span, &mut PanicOnIssue), Err(failed));
}

#[test]
fn malformed_envelope_entries_are_counted_in_band() {
    let mut span = valid_envelope_span(1, 1);
    span.insert("events".into(), vrl::value!(["bad", {"name": "e"}]));
    span.insert(
        "attributes".into(),
        vrl::value!(["bad", {"key": "nested", "value": {"kvlistValue": {"values": ["bad"]}}}]),
    );
    let envelope = envelope(vec![
        resource_spans_value(vec![
            Value::from("bad"),
            scope_spans_value(vec![Value::Object(span)]),
        ]),
        Value::from("bad"),
    ]);

    assert_eq!(
        convert_hinted(envelope.clone()),
        (
            legacy_span(
                &envelope,
                Span {
                    attributes: attrs([("nested", AttrValue::Map(AttrMap::new()))]),
                    events: vec![SpanEvent {
                        name: "e".into(),
                        time: DateTime::<Utc>::UNIX_EPOCH,
                        attributes: Attributes::new(),
                        dropped_attributes_count: DroppedCount::ZERO,
                    }],
                    dropped_attributes_count: DroppedCount::new(2),
                    dropped_events_count: DroppedCount::new(1),
                    ..Span::new(sid(1), "s")
                },
            ),
            TraceConversionCounts {
                malformed_groupings: 2,
                malformed_events: 1,
                malformed_attributes: 2,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn malformed_flattened_entries_are_counted_in_band() {
    let mut span = flattened_legacy();
    span.insert(vrl::event_path!("events"), vrl::value!(["bad"]));
    span.insert(
        vrl::event_path!(spans::ATTRIBUTES_KEY),
        vrl::value!(["bad"]),
    );
    span.insert(vrl::event_path!(spans::RESOURCE_KEY), vrl::value!("bad"));

    let mut expected = legacy_span(
        &span,
        Span {
            dropped_attributes_count: DroppedCount::new(1),
            dropped_events_count: DroppedCount::new(1),
            ..Span::new(sid(1), "s")
        },
    );
    expected[0].resource_mut().dropped_attributes_count = DroppedCount::new(1);
    assert_eq!(
        convert_hinted(span.clone()),
        (
            expected,
            TraceConversionCounts {
                malformed_events: 1,
                malformed_attributes: 2,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn envelope_accepts_raw_ids() {
    let mut span = valid_envelope_span(1, 1);
    span.insert("parentSpanId".into(), raw_span_id(3));
    let envelope = envelope_with_spans(vec![Value::Object(span)]);
    assert_eq!(
        convert_hinted(envelope.clone()),
        traces(legacy_span(
            &envelope,
            Span {
                parent_span_id: Some(sid(3)),
                ..Span::new(sid(1), "s")
            },
        ))
    );
}

#[test]
fn envelope_accepts_string_backed_raw_ids() {
    let envelope = envelope_with_spans(vec![Value::Object(envelope_span(
        Value::from("0123456789abcdef"),
        Value::from("01234567"),
    ))]);
    let expected_trace_id = TraceId::from_bytes(*b"0123456789abcdef").unwrap();
    let expected_span_id = SpanId::from_bytes(*b"01234567").unwrap();

    assert_eq!(
        convert_hinted(envelope.clone()),
        traces(vec![typed_event_with_metadata(
            expected_trace_id.get(),
            vec![Span::new(expected_span_id, "s")],
            envelope.metadata(),
        )])
    );
}

#[test]
fn envelope_rejects_and_counts_malformed_ids() {
    let with_parent = |parent: Value| {
        let mut span = valid_envelope_span(1, 1);
        span.insert("parentSpanId".into(), parent);
        span
    };
    let spans = [
        envelope_span(
            Value::from("00000000000000000000000000000001"),
            raw_span_id(1),
        ),
        envelope_span(raw_trace_id(1), Value::from("0000000000000001")),
        with_parent(Value::from("0000000000000003")),
        envelope_span(Value::Integer(1), raw_span_id(1)),
        envelope_span(raw_trace_id(1), Value::from("abc")),
        with_parent(Value::Integer(7)),
    ];
    let mut values: Vec<Value> = spans.into_iter().map(Value::Object).collect();
    values.push(Value::from("not a span"));
    let count = values.len();
    assert_eq!(convert_hinted(envelope_with_spans(values)), rejected(count));
}

#[test]
fn envelope_reads_span_and_link_flags() {
    let link = ObjectMap::from([
        ("traceId".into(), raw_trace_id(2)),
        ("spanId".into(), raw_span_id(2)),
        ("flags".into(), Value::Integer(0x301)),
    ]);
    let mut span = valid_envelope_span(1, 1);
    span.insert("flags".into(), Value::Integer(0x101));
    span.insert("links".into(), Value::Array(vec![Value::Object(link)]));
    let envelope = envelope_with_spans(vec![Value::Object(span)]);
    assert_eq!(
        convert_hinted(envelope.clone()),
        traces(legacy_span(
            &envelope,
            Span {
                flags: OtlpSpanFlags::from(0x101),
                links: vec![SpanLink {
                    flags: OtlpSpanFlags::from(0x301),
                    ..typed_link(2, 2)
                }],
                ..Span::new(sid(1), "s")
            },
        ))
    );
}

#[test]
fn flattened_empty_parent_is_none_and_hex_parent_decodes() {
    for (parent, parent_span_id) in [("", None), ("0000000000000009", Some(sid(9)))] {
        let mut span = flattened_legacy();
        span.insert(vrl::event_path!("parent_span_id"), Value::from(parent));
        assert_eq!(
            convert_hinted(span.clone()),
            traces(legacy_span(
                &span,
                Span {
                    parent_span_id,
                    ..Span::new(sid(1), "s")
                },
            )),
            "{parent:?}"
        );
    }
}

#[test]
fn flattened_invalid_ids_reject_span() {
    for (key, value) in [
        (spans::TRACE_ID_KEY, "0000000000000001"),
        (spans::SPAN_ID_KEY, "zz"),
        ("parent_span_id", "abcdef"),
    ] {
        let mut span = flattened_legacy();
        span.insert(vrl::event_path!(key), Value::from(value));
        assert_eq!(
            convert_hinted(span.clone()),
            rejected(1),
            "{key} = {value:?}"
        );
    }
}

#[test]
fn envelope_reads_timestamps_beyond_i64_nanos() {
    let mut span = valid_envelope_span(1, 1);
    span.insert(
        "startTimeUnixNano".into(),
        Value::Integer((u64::MAX - 1).cast_signed()),
    );
    span.insert(
        "endTimeUnixNano".into(),
        Value::Integer(u64::MAX.cast_signed()),
    );
    let (events, counts) = convert_hinted(envelope_with_spans(vec![Value::Object(span)]));
    let span = &events[0].spans()[0];
    assert_eq!(
        (span.start_time, span.duration, counts),
        (
            DateTime::from_timestamp(18_446_744_073, 709_551_614).unwrap(),
            Duration::from_nanos(1),
            TraceConversionCounts::default(),
        )
    );
}

#[test]
fn flattened_reads_timestamps_beyond_i64_nanos() {
    let rs = ResourceSpans::from_spans(vec![PbSpan {
        start_time_unix_nano: u64::MAX - 1,
        end_time_unix_nano: u64::MAX,
        events: vec![PbEvent {
            time_unix_nano: u64::MAX,
            ..PbEvent::default()
        }],
        ..PbSpan::from_parts(1, 1, "s")
    }]);
    let event = rs.into_event_iter().next().unwrap().into_trace();
    let (events, counts) = convert_hinted(event);
    let span = &events[0].spans()[0];
    assert_eq!(
        (span.start_time, span.duration, span.events[0].time, counts),
        (
            DateTime::from_timestamp(18_446_744_073, 709_551_614).unwrap(),
            Duration::from_nanos(1),
            DateTime::from_timestamp(18_446_744_073, 709_551_615).unwrap(),
            TraceConversionCounts::default(),
        )
    );
}

/// Returns `object` with `key` set to `value`.
fn with_key(object: Value, key: &str, value: Value) -> Value {
    let Value::Object(mut object) = object else {
        panic!("expected an object");
    };
    object.insert(key.into(), value);
    Value::Object(object)
}

#[test]
fn envelope_reports_unmapped_fields() {
    let mut span = valid_envelope_span(1, 1);
    span.insert("extra".into(), Value::Integer(1));
    // The protobuf decoder writes only JSON names, so the `snake_case` spelling is unmapped.
    span.insert("span_id".into(), raw_span_id(1));
    let scope_spans = scope_spans_value(vec![Value::Object(span)]);
    let resource_spans = with_key(
        resource_spans_value(vec![scope_spans]),
        "extra",
        Value::Integer(2),
    );
    let mut envelope = envelope(vec![resource_spans]);
    envelope.insert(vrl::event_path!("other"), Value::Integer(3));

    assert_eq!(
        convert_hinted(envelope.clone()),
        (
            legacy_span(&envelope, Span::new(sid(1), "s")),
            TraceConversionCounts {
                unmapped_fields: 4,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn flattened_reports_unmapped_fields_except_ingest_timestamp() {
    let mut span = flattened_legacy();
    span.insert(
        vrl::event_path!("ingest_timestamp"),
        Value::from(DateTime::<Utc>::UNIX_EPOCH),
    );
    span.insert(vrl::event_path!("custom"), Value::from("x"));
    span.insert(
        vrl::event_path!("status"),
        vrl::value!({"code": 2, "message": "boom", "extra": true}),
    );
    span.insert(
        vrl::event_path!("events"),
        vrl::value!([{"name": "e", "extra": 1}]),
    );

    assert_eq!(
        convert_hinted(span.clone()),
        (
            legacy_span(
                &span,
                Span {
                    status: SpanStatus::Error("boom".into()),
                    events: vec![SpanEvent {
                        name: "e".into(),
                        time: DateTime::<Utc>::UNIX_EPOCH,
                        attributes: Attributes::new(),
                        dropped_attributes_count: DroppedCount::ZERO,
                    }],
                    ..Span::new(sid(1), "s")
                },
            ),
            TraceConversionCounts {
                unmapped_fields: 3,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn envelope_reports_and_defaults_wrong_typed_fields() {
    let mut span = valid_envelope_span(1, 1);
    span.insert("name".into(), Value::Integer(42));
    span.insert("kind".into(), Value::from("SPAN_KIND_NOPE"));
    span.insert("startTimeUnixNano".into(), Value::from("10"));
    span.insert("status".into(), Value::from("bad"));
    let scope_spans = with_key(
        scope_spans_value(vec![Value::Object(span)]),
        "scope",
        Value::Integer(5),
    );
    let resource_spans = with_key(
        resource_spans_value(vec![scope_spans]),
        "resource",
        Value::from("bad"),
    );
    let envelope = envelope(vec![resource_spans]);

    assert_eq!(
        convert_hinted(envelope.clone()),
        (
            legacy_span(&envelope, Span::new(sid(1), "")),
            TraceConversionCounts {
                malformed_fields: 6,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn flattened_reports_and_defaults_wrong_typed_fields() {
    let mut span = flattened_legacy();
    span.insert(vrl::event_path!("name"), Value::Integer(42));
    span.insert(vrl::event_path!("kind"), Value::from("server"));
    span.insert(
        vrl::event_path!(spans::DROPPED_ATTRIBUTES_COUNT_KEY),
        Value::Integer(-1),
    );
    // The source writes timestamps, so an integer is not coerced.
    span.insert(vrl::event_path!("start_time_unix_nano"), Value::Integer(5));

    assert_eq!(
        convert_hinted(span.clone()),
        (
            legacy_span(&span, Span::new(sid(1), "")),
            TraceConversionCounts {
                malformed_fields: 4,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn envelope_absent_attribute_key_matches_an_empty_wire_key() {
    let mut span = valid_envelope_span(1, 1);
    span.insert(
        "attributes".into(),
        vrl::value!([{"value": {"stringValue": "kept"}}]),
    );
    let (events, counts) = convert_hinted(envelope_with_spans(vec![Value::Object(span)]));

    let (wire_events, wire_counts) = ResourceSpans::from_spans(vec![PbSpan {
        trace_id: tid(1).to_bytes().to_vec(),
        span_id: sid(1).to_bytes().to_vec(),
        name: "s".into(),
        attributes: vec![KeyValue::from_string("", "kept")],
        ..PbSpan::default()
    }])
    .decode();

    assert_eq!(
        events[0].spans()[0].attributes,
        wire_events[0].spans()[0].attributes
    );
    assert_eq!(
        events[0].spans()[0].attributes,
        attrs([("", AttrValue::String("kept".into()))])
    );
    assert_eq!(
        counts.malformed_attributes,
        wire_counts.malformed_attributes
    );
    assert_eq!(counts.malformed_attributes, 0);
}

#[test]
fn envelope_drops_and_counts_malformed_attribute_values() {
    let mut span = valid_envelope_span(1, 1);
    span.insert(
        "attributes".into(),
        vrl::value!([
            {"key": "a", "value": {"intValue": "12"}},
            {"key": 1, "value": {"stringValue": "nope"}},
            {"key": "list", "value": {"arrayValue": {"values": [{"intValue": 1}, "bad"]}}},
        ]),
    );
    let resource_spans = with_key(
        resource_spans_value(vec![scope_spans_value(vec![Value::Object(span)])]),
        "resource",
        vrl::value!({"attributes": {"k": "v"}}),
    );
    let envelope = envelope(vec![resource_spans]);

    let mut expected = legacy_span(
        &envelope,
        Span {
            attributes: attrs([("list", AttrValue::Array(vec![AttrValue::Integer(1)]))]),
            dropped_attributes_count: DroppedCount::new(3),
            ..Span::new(sid(1), "s")
        },
    );
    // An object is not an attribute list, so it counts as one malformed entry.
    expected[0].resource_mut().dropped_attributes_count = DroppedCount::new(1);
    assert_eq!(
        convert_hinted(envelope),
        (
            expected,
            TraceConversionCounts {
                malformed_attributes: 4,
                ..TraceConversionCounts::default()
            },
        )
    );
}

#[test]
fn flattened_drops_and_counts_attribute_values_with_no_otlp_form() {
    let mut span = flattened_legacy();
    span.insert(
        vrl::event_path!(spans::ATTRIBUTES_KEY),
        Value::Object(ObjectMap::from([
            ("ok".into(), Value::from("v")),
            ("when".into(), Value::from(DateTime::<Utc>::UNIX_EPOCH)),
            (
                "list".into(),
                Value::Array(vec![
                    Value::Integer(1),
                    Value::from(DateTime::<Utc>::UNIX_EPOCH),
                ]),
            ),
        ])),
    );

    assert_eq!(
        convert_hinted(span.clone()),
        (
            legacy_span(
                &span,
                Span {
                    attributes: attrs([
                        ("list", AttrValue::Array(vec![AttrValue::Integer(1)])),
                        ("ok", AttrValue::String("v".into())),
                    ]),
                    dropped_attributes_count: DroppedCount::new(2),
                    ..Span::new(sid(1), "s")
                },
            ),
            TraceConversionCounts {
                malformed_attributes: 2,
                ..TraceConversionCounts::default()
            },
        )
    );
}
