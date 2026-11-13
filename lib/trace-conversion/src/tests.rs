use chrono::{DateTime, Utc};
use similar_asserts::assert_eq;
use vector_core::event::{
    TraceEvent as LegacyTraceEvent, TraceLayout, typed_trace::TraceConversionCounts,
};
use vrl::{
    event_path,
    value::{ObjectMap, Value},
};

use super::{SelectError, convert_legacy, convert_or_retain, datadog};

const UNKNOWN_HINT: TraceLayout = TraceLayout::Unrecognized(99);

fn assert_no_issues(reporter: &TraceConversionCounts) {
    assert_eq!(reporter, &TraceConversionCounts::default());
}

fn datadog_legacy() -> LegacyTraceEvent {
    let mut map = ObjectMap::new();
    map.insert("priority".into(), Value::from(1i64));
    map.insert("language_name".into(), Value::from("rust"));
    map.insert("spans".into(), Value::Array(Vec::new()));
    let mut event = LegacyTraceEvent::from(map);
    event.metadata_mut().set_trace_layout(TraceLayout::Datadog);
    event
}

fn otlp_flattened() -> LegacyTraceEvent {
    let mut event = LegacyTraceEvent::default();
    event.insert(
        event_path!("trace_id"),
        Value::from("00000000000000000000000000000001"),
    );
    event.insert(event_path!("span_id"), Value::from("0000000000000001"));
    event.insert(event_path!("name"), Value::from("s"));
    event.insert(
        event_path!("start_time_unix_nano"),
        Value::from(DateTime::<Utc>::UNIX_EPOCH),
    );
    event
}

fn ambiguous_legacy() -> LegacyTraceEvent {
    let mut map = ObjectMap::new();
    map.insert("priority".into(), Value::from(1i64));
    map.insert("spans".into(), Value::Array(Vec::new()));
    map.insert(
        "resourceSpans".into(),
        Value::Array(vec![Value::Object(ObjectMap::from([(
            "scopeSpans".into(),
            Value::Array(Vec::new()),
        )]))]),
    );
    LegacyTraceEvent::from(map)
}

#[test]
fn empty_datadog_chunk_is_success() {
    let mut reporter = TraceConversionCounts::default();
    let typed = convert_legacy(&datadog_legacy(), &mut reporter).unwrap();
    assert_eq!(typed, Vec::new());
    assert_no_issues(&reporter);
}

#[test]
fn unknown_hint_does_not_detect() {
    let mut legacy = datadog_legacy();
    legacy.metadata_mut().set_trace_layout(UNKNOWN_HINT);
    let mut reporter = TraceConversionCounts::default();

    assert_eq!(
        convert_legacy(&legacy, &mut reporter),
        Err(SelectError::UnknownHint)
    );
    assert_no_issues(&reporter);
}

#[test]
fn convert_or_retain_returns_original_on_unknown_hint() {
    let mut legacy = datadog_legacy();
    legacy.metadata_mut().set_trace_layout(UNKNOWN_HINT);
    let mut reporter = TraceConversionCounts::default();
    let returned = convert_or_retain(legacy.clone(), &mut reporter).unwrap_err();

    assert_eq!(returned, legacy);
    assert_no_issues(&reporter);
}

#[test]
fn convert_or_retain_empty_typed_output_is_success() {
    let mut reporter = TraceConversionCounts::default();

    assert_eq!(
        convert_or_retain(datadog_legacy(), &mut reporter).unwrap(),
        Vec::new()
    );
    assert_no_issues(&reporter);
}

#[test]
fn detectors_are_mutually_exclusive_for_typical_layouts() {
    let datadog = {
        let mut event = datadog_legacy();
        event.metadata_mut().clear_trace_layout();
        event
    };
    let otlp = otlp_flattened();
    assert!(datadog::is_historical_datadog_layout(&datadog));
    assert!(!opentelemetry_proto::typed_trace::is_historical_otlp_layout(&datadog));
    assert!(!datadog::is_historical_datadog_layout(&otlp));
    assert!(opentelemetry_proto::typed_trace::is_historical_otlp_layout(
        &otlp
    ));
    let mut datadog_reporter = TraceConversionCounts::default();
    assert!(convert_legacy(&datadog, &mut datadog_reporter).is_ok());
    assert_no_issues(&datadog_reporter);

    let mut otlp_reporter = TraceConversionCounts::default();
    assert!(convert_legacy(&otlp, &mut otlp_reporter).is_ok());
    assert_no_issues(&otlp_reporter);
}

#[test]
fn ambiguous_unhinted_layout_errors() {
    assert!(datadog::is_historical_datadog_layout(&ambiguous_legacy()));
    assert!(opentelemetry_proto::typed_trace::is_historical_otlp_layout(
        &ambiguous_legacy()
    ));
    let mut reporter = TraceConversionCounts::default();

    assert_eq!(
        convert_legacy(&ambiguous_legacy(), &mut reporter),
        Err(SelectError::AmbiguousLayout)
    );
    assert_no_issues(&reporter);
}

#[test]
fn recognized_hint_skips_detection() {
    let mut mixed = ambiguous_legacy();
    mixed.metadata_mut().set_trace_layout(TraceLayout::Datadog);
    let mut datadog_reporter = TraceConversionCounts::default();
    assert_eq!(
        convert_legacy(&mixed, &mut datadog_reporter).unwrap(),
        Vec::new()
    );
    assert_no_issues(&datadog_reporter);

    mixed
        .metadata_mut()
        .set_trace_layout(TraceLayout::OtlpResourceSpans);
    let mut otlp_reporter = TraceConversionCounts::default();
    assert_eq!(
        convert_legacy(&mixed, &mut otlp_reporter).unwrap(),
        Vec::new()
    );
    assert_eq!(otlp_reporter.discarded_groupings, 1);
}

#[test]
fn forwards_otlp_conversion_issues_to_reporter() {
    let mut legacy = otlp_flattened();
    legacy.insert(event_path!("span_id"), Value::from("0000000000000000"));
    let mut reporter = TraceConversionCounts::default();

    assert_eq!(convert_legacy(&legacy, &mut reporter).unwrap(), Vec::new());
    assert_eq!(reporter.rejected_spans, 1);

    let mut hinted = otlp_flattened();
    hinted
        .metadata_mut()
        .set_trace_layout(TraceLayout::OtelFlattened);
    hinted.insert(event_path!("span_id"), Value::from("0000000000000000"));
    let mut hinted_reporter = TraceConversionCounts::default();

    assert_eq!(
        convert_legacy(&hinted, &mut hinted_reporter).unwrap(),
        Vec::new()
    );
    assert_eq!(hinted_reporter.rejected_spans, 1);

    let mut wrapped = otlp_flattened();
    wrapped
        .metadata_mut()
        .set_trace_layout(TraceLayout::OtelFlattened);
    wrapped.insert(event_path!("span_id"), Value::from("0000000000000000"));
    let mut wrapped_reporter = TraceConversionCounts::default();

    assert_eq!(
        convert_or_retain(wrapped, &mut wrapped_reporter).unwrap(),
        Vec::new()
    );
    assert_eq!(wrapped_reporter.rejected_spans, 1);
}
