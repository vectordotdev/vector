use super::*;
use crate::config::log_schema;
use bytes::{Buf, BufMut, BytesMut};
use chrono::TimeZone;
use prost::Message;
use quickcheck::{QuickCheck, TestResult};
use regex::Regex;
use similar_asserts::assert_eq;
use vector_buffers::encoding::Encodable;
use vrl::event_path;

use crate::event::event_exceeds_max_nesting_cost;
use crate::event::ser::{
    ARRAY_FRAME_COST, MAX_VALUE_NESTING_FRAMES, OBJECT_FRAME_COST, TIMESTAMP_FRAME_COST,
    check_value_nesting_cost,
};
use vector_buffers::Bufferable;

fn encode_value<T: Encodable, B: BufMut>(value: T, buffer: &mut B) {
    value.encode(buffer).expect("encoding should not fail");
}

fn decode_value<T: Encodable, B: Buf + Clone>(buffer: B) -> T {
    T::decode(T::get_metadata(), buffer).expect("decoding should not fail")
}

// Ser/De the EventArray never loses bytes
#[test]
fn serde_eventarray_no_size_loss() {
    fn inner(events: EventArray) -> TestResult {
        let expected = events.clone();

        let mut buffer = BytesMut::with_capacity(64);
        encode_value(events, &mut buffer);

        let actual = decode_value::<EventArray, _>(buffer);
        assert_eq!(actual.size_of(), expected.size_of());

        TestResult::passed()
    }

    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(EventArray) -> TestResult);
}

// Ser/De the EventArray type through EncodeBytes -> DecodeBytes
#[test]
#[allow(clippy::neg_cmp_op_on_partial_ord)] // satisfying clippy leads to less
// clear expression
fn back_and_forth_through_bytes() {
    fn inner(events: EventArray) -> TestResult {
        let expected = events.clone();

        let mut buffer = BytesMut::with_capacity(64);
        encode_value(events, &mut buffer);

        let actual = decode_value::<EventArray, _>(buffer);

        assert_eq!(expected, actual);

        TestResult::passed()
    }

    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(EventArray) -> TestResult);
}

#[test]
fn serialization() {
    let mut event = LogEvent::from("raw log line");
    event.insert(event_path!("foo"), "bar");
    event.insert(event_path!("bar"), "baz");

    let expected_all = serde_json::json!({
        "message": "raw log line",
        "foo": "bar",
        "bar": "baz",
        "timestamp": event.get(log_schema().timestamp_key_target_path().unwrap()),
    });

    let actual_all = serde_json::to_value(event.all_event_fields().unwrap()).unwrap();
    assert_eq!(expected_all, actual_all);

    let rfc3339_re = Regex::new(r"\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z\z").unwrap();
    assert!(rfc3339_re.is_match(actual_all.pointer("/timestamp").unwrap().as_str().unwrap()));
}

#[test]
fn type_serialization() {
    use serde_json::json;

    let mut event = LogEvent::from("hello world");
    event.insert(event_path!("int"), 4);
    event.insert(event_path!("float"), 5.5);
    event.insert(event_path!("bool"), true);
    event.insert(event_path!("string"), "thisisastring");

    let map = serde_json::to_value(event.all_event_fields().unwrap()).unwrap();
    assert_eq!(map["float"], json!(5.5));
    assert_eq!(map["int"], json!(4));
    assert_eq!(map["bool"], json!(true));
    assert_eq!(map["string"], json!("thisisastring"));
}

// ---------------------------------------------------------------------------
// Nesting validation tests
// ---------------------------------------------------------------------------
//
// Prost enforces a decode recursion limit of 100 (no limit on encode). Each nesting
// level consumes a path-dependent number of prost recursion frames:
//
//   - `Value::Object` level: Value + ValueMap + map_entry = 3 frames
//   - `Value::Array` level:  Value + ValueArray          = 2 frames
//
// Encoding paths have different fixed proto-wrapper overhead before the Value tree:
//
//   - `Log.fields` and `Trace.fields` can carry 99 Value frames.
//   - `Log.value` and metadata can carry 96 Value frames.
//
// The gate uses the highest common safe limit, MAX_VALUE_NESTING_FRAMES (96), for every
// arbitrary Value. The boundary tests verify both that common limit and the extra
// headroom on the wider wire paths.
//
// The saturated-event tests create events with ALL Value-carrying fields at the common
// max frame cost simultaneously. The proto conversion code populates every
// field (including deprecated ones like Log.metadata), so a single roundtrip per event
// type covers every proto path automatically.

/// Maximum number of object-only nesting levels that fit the common Value budget.
const MAX_OBJECT_DEPTH_VALUE: usize = MAX_VALUE_NESTING_FRAMES / OBJECT_FRAME_COST;

/// Maximum number of array-only nesting levels that fit the common Value budget.
const MAX_ARRAY_DEPTH_VALUE: usize = MAX_VALUE_NESTING_FRAMES / ARRAY_FRAME_COST;

/// Creates a Value with the specified number of nested Object wrapping levels.
///
/// Returns a Value that is `wrapping_levels` nested Objects deep, with a string leaf.
fn create_nested_value(wrapping_levels: usize) -> Value {
    let mut value = Value::from("innermost");
    for _ in 0..wrapping_levels {
        let mut map = ObjectMap::new();
        map.insert("nested".into(), value);
        value = Value::Object(map);
    }
    value
}

/// Creates a Value with the specified number of nested Array wrapping levels.
fn create_nested_array(wrapping_levels: usize) -> Value {
    let mut value = Value::from("innermost");
    for _ in 0..wrapping_levels {
        value = Value::Array(vec![value]);
    }
    value
}

/// Creates a Value with the specified number of nested Object wrapping levels around
/// the supplied leaf. Used to probe leaf-specific frame costs (e.g. `Value::Timestamp`).
fn create_nested_value_with_leaf(wrapping_levels: usize, leaf: Value) -> Value {
    let mut value = leaf;
    for _ in 0..wrapping_levels {
        let mut map = ObjectMap::new();
        map.insert("nested".into(), value);
        value = Value::Object(map);
    }
    value
}

/// A fixed [`Value::Timestamp`] for use as a leaf in nesting tests.
fn ts_leaf() -> Value {
    Value::Timestamp(
        chrono::Utc
            .timestamp_opt(1_700_000_000, 0)
            .single()
            .unwrap(),
    )
}

/// Create a [`LogEvent`] with every arbitrary Value at `value_depth`.
fn create_saturated_log(value_depth: usize) -> LogEvent {
    let mut event = LogEvent::default();
    event.insert(event_path!("data"), create_nested_value(value_depth - 1));
    *event.metadata_mut().value_mut() = create_nested_value(value_depth);
    event
}

/// Create a [`TraceEvent`] with every arbitrary Value at `value_depth`.
fn create_saturated_trace(value_depth: usize) -> TraceEvent {
    let mut trace = TraceEvent::default();
    trace.insert(event_path!("data"), create_nested_value(value_depth - 1));
    *trace.metadata_mut().value_mut() = create_nested_value(value_depth);
    trace
}

/// Create a Metric with metadata at `value_depth`.
/// (Metric values have fixed structure — only metadata carries arbitrary Values.)
fn create_saturated_metric(value_depth: usize) -> Metric {
    let mut metric = Metric::new(
        "test",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    );
    *metric.metadata_mut().value_mut() = create_nested_value(value_depth);
    metric
}

/// Build all three `EventArray` variants with every arbitrary Value at the same depth.
fn saturated_event_arrays(value_depth: usize) -> Vec<(&'static str, EventArray)> {
    vec![
        (
            "Log",
            EventArray::Logs(LogArray::from(vec![create_saturated_log(value_depth)])),
        ),
        (
            "Trace",
            EventArray::Traces(TraceArray::from(vec![create_saturated_trace(value_depth)])),
        ),
        (
            "Metric",
            EventArray::Metrics(MetricArray::from(vec![create_saturated_metric(
                value_depth,
            )])),
        ),
    ]
}

/// Build all three Event variants for `EventWrapper` encoding.
fn saturated_events(value_depth: usize) -> Vec<(&'static str, Event)> {
    vec![
        ("Log", Event::Log(create_saturated_log(value_depth))),
        ("Trace", Event::Trace(create_saturated_trace(value_depth))),
        (
            "Metric",
            Event::Metric(create_saturated_metric(value_depth)),
        ),
    ]
}

/// Verify that the common Value budget roundtrips through every protobuf path and that
/// increasing every Value by one object level exceeds at least one wire-path limit.
#[test]
fn max_nesting_budget_is_safe_for_all_paths() {
    for (name, array) in saturated_event_arrays(MAX_OBJECT_DEPTH_VALUE) {
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        assert!(
            proto::EventArray::decode(buf.freeze()).is_ok(),
            "EventArray decode FAILED for {name} at the common Value budget.",
        );
    }

    for (name, event) in saturated_events(MAX_OBJECT_DEPTH_VALUE) {
        let wrapper = proto::EventWrapper::from(event);
        let mut buf = BytesMut::with_capacity(65536);
        wrapper.encode(&mut buf).unwrap();
        assert!(
            proto::EventWrapper::decode(buf.freeze()).is_ok(),
            "EventWrapper decode FAILED for {name} at the common Value budget.",
        );
    }

    let any_fails = saturated_event_arrays(MAX_OBJECT_DEPTH_VALUE + 1)
        .into_iter()
        .any(|(_, array)| {
            let proto_array = proto::EventArray::from(array);
            let mut buf = BytesMut::with_capacity(65536);
            proto_array.encode(&mut buf).unwrap();
            proto::EventArray::decode(buf.freeze()).is_err()
        });
    assert!(
        any_fails,
        "No path failed one object level above MAX_VALUE_NESTING_FRAMES.",
    );
}

/// Verify the nesting gate accepts all event types at the max object-only depth.
#[test]
fn nesting_gate_accepts_all_types_at_max_depth() {
    for (name, array) in saturated_event_arrays(MAX_OBJECT_DEPTH_VALUE) {
        let mut buf = BytesMut::with_capacity(65536);
        assert!(
            array.encode(&mut buf).is_ok(),
            "nesting gate rejected {name} at max object depths",
        );
    }
}

/// Verify the nesting gate rejects every event type above the common Value budget.
#[test]
fn nesting_gate_rejects_above_max_depth() {
    for (name, array) in saturated_event_arrays(MAX_OBJECT_DEPTH_VALUE + 1) {
        let mut buf = BytesMut::with_capacity(65536);
        assert!(
            matches!(
                array.encode(&mut buf),
                Err(super::super::ser::EncodeError::NestingTooDeep { .. })
            ),
            "nesting gate should reject {name} above the common Value budget",
        );
    }
}

/// Verify that the wider `Log.fields` path has one level of headroom over the common
/// budget while the metadata path is tight, for both object-only and array-only values.
///
/// Object-only `Log.fields`:     depth 33 succeeds, 34 fails.
/// Object-only `metadata_full`:  depth 32 succeeds, 33 fails.
/// Array-only  `Log.fields`:     depth 49 succeeds, 50 fails.
/// Array-only  `metadata_full`:  depth 48 succeeds, 49 fails.
#[test]
fn per_path_boundaries() {
    let roundtrip_value = |value: Value| -> bool {
        let mut event = LogEvent::default();
        event.insert(event_path!("data"), value);
        let array = EventArray::Logs(LogArray::from(vec![event]));
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        proto::EventArray::decode(buf.freeze()).is_ok()
    };

    let roundtrip_metadata = |value: Value| -> bool {
        let mut event = LogEvent::from("flat");
        *event.metadata_mut().value_mut() = value;
        let array = EventArray::Logs(LogArray::from(vec![event]));
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        proto::EventArray::decode(buf.freeze()).is_ok()
    };

    // `Log.fields` accepts 33 object levels (cost 99), one more than the common limit.
    // The "data" key contributes the outer object level.
    assert!(
        roundtrip_value(create_nested_value(MAX_OBJECT_DEPTH_VALUE)),
        "Log.fields should succeed one object level above the common budget"
    );
    assert!(
        !roundtrip_value(create_nested_value(MAX_OBJECT_DEPTH_VALUE + 1)),
        "Log.fields should fail at object depth {}",
        MAX_OBJECT_DEPTH_VALUE + 2
    );

    // `metadata_full` is tight at the common limit of 32 object levels (cost 96).
    assert!(
        roundtrip_metadata(create_nested_value(MAX_OBJECT_DEPTH_VALUE)),
        "metadata_full should succeed at the common object-depth limit"
    );
    assert!(
        !roundtrip_metadata(create_nested_value(MAX_OBJECT_DEPTH_VALUE + 1)),
        "metadata_full should fail at object depth {}",
        MAX_OBJECT_DEPTH_VALUE + 1
    );

    // The outer object plus 48 nested arrays costs 99 frames on `Log.fields`.
    assert!(
        roundtrip_value(create_nested_array(MAX_ARRAY_DEPTH_VALUE)),
        "Log.fields should succeed with one array level of headroom"
    );
    assert!(
        !roundtrip_value(create_nested_array(MAX_ARRAY_DEPTH_VALUE + 1)),
        "Log.fields should fail at array depth {}",
        MAX_ARRAY_DEPTH_VALUE + 2
    );

    // `metadata_full` is tight at 48 array levels (cost 96).
    assert!(
        roundtrip_metadata(create_nested_array(MAX_ARRAY_DEPTH_VALUE)),
        "metadata_full should succeed at the common array-depth limit"
    );
    assert!(
        !roundtrip_metadata(create_nested_array(MAX_ARRAY_DEPTH_VALUE + 1)),
        "metadata_full should fail at array depth {}",
        MAX_ARRAY_DEPTH_VALUE + 1
    );
}

/// Non-object log roots are encoded through `Log.value`, not the legacy `Log.fields`
/// map. Its lower wire limit establishes the common budget used for every Value.
#[test]
fn value_budget_matches_tightest_wire_path() {
    let make_log = |array_depth| LogEvent::from(create_nested_array(array_depth));
    let raw_roundtrip = |log: LogEvent| {
        let array = EventArray::Logs(LogArray::from(vec![log]));
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        proto::EventArray::decode(buf.freeze()).is_ok()
    };

    assert!(
        raw_roundtrip(make_log(MAX_ARRAY_DEPTH_VALUE)),
        "Log.value should roundtrip at its array-depth limit",
    );
    assert!(
        !raw_roundtrip(make_log(MAX_ARRAY_DEPTH_VALUE + 1)),
        "Log.value should fail prost decoding past its array-depth limit",
    );

    let accepted = Event::Log(make_log(MAX_ARRAY_DEPTH_VALUE));
    assert!(event_exceeds_max_nesting_cost(&accepted).is_none());
    let accepted = EventArray::Logs(LogArray::from(vec![accepted.into_log()]));
    let mut buf = BytesMut::with_capacity(65536);
    accepted
        .encode(&mut buf)
        .expect("the last decodable Log.value depth should pass the gate");

    let rejected = Event::Log(make_log(MAX_ARRAY_DEPTH_VALUE + 1));
    assert_eq!(
        event_exceeds_max_nesting_cost(&rejected),
        Some((98, MAX_VALUE_NESTING_FRAMES)),
    );
    let rejected = EventArray::Logs(LogArray::from(vec![rejected.into_log()]));
    assert!(
        rejected.clone().filter_unencodable().is_none(),
        "the buffer filter should drop an undecodable Log.value root",
    );
    let mut buf = BytesMut::with_capacity(65536);
    assert!(
        matches!(
            rejected.encode(&mut buf),
            Err(super::super::ser::EncodeError::NestingTooDeep {
                cost: 98,
                budget: MAX_VALUE_NESTING_FRAMES,
            })
        ),
        "the encode-time gate should reject an undecodable Log.value root",
    );
}

/// Verify that array-only nesting deeper than the object-only cap (32) is accepted by
/// the gate — this is the regression that the frame-cost check addresses. Previously a
/// uniform depth-33 cap dropped array-only events that prost would happily roundtrip.
#[test]
fn nesting_gate_accepts_deep_array_nesting() {
    // Forty arrays below the outer log object cost 83 frames, comfortably under the
    // 96-frame Value budget but over the old uniform depth limit.
    let mut event = LogEvent::default();
    event.insert(event_path!("data"), create_nested_array(40));
    let array = EventArray::Logs(LogArray::from(vec![event]));
    let mut buf = BytesMut::with_capacity(65536);
    assert!(
        array.encode(&mut buf).is_ok(),
        "nesting gate should accept array-only nesting at depth 40",
    );
}

/// Verify the gate correctly accounts for mixed array/object nesting via the per-variant
/// frame weights. Uses the metadata path because it has no outer wrapping object, making
/// the arithmetic match the inserted Value's cost directly.
#[test]
fn nesting_gate_handles_mixed_array_object_nesting() {
    // Alternating levels (innermost-Array, then Object, then Array, ...). For N levels,
    // cost = ceil(N/2)*ARRAY_FRAME_COST + floor(N/2)*OBJECT_FRAME_COST.
    let build_alternating = |total_levels: usize| -> Value {
        let mut value = Value::from("leaf");
        for i in 0..total_levels {
            if i.is_multiple_of(2) {
                value = Value::Array(vec![value]);
            } else {
                let mut map = ObjectMap::new();
                map.insert("k".into(), value);
                value = Value::Object(map);
            }
        }
        value
    };

    // 38 alternating levels: 19 array (cost 38) + 19 object (cost 57) = 95 frames.
    // Under the common Value budget of 96. Fits.
    let mut event = LogEvent::from("flat");
    *event.metadata_mut().value_mut() = build_alternating(38);
    let array = EventArray::Logs(LogArray::from(vec![event]));
    let mut buf = BytesMut::with_capacity(65536);
    assert!(
        array.encode(&mut buf).is_ok(),
        "nesting gate should accept 38 alternating metadata levels (cost 95)",
    );

    // 39 alternating levels: 20 array (cost 40) + 19 object (cost 57) = 97 frames.
    // Over the common Value budget of 96. Fails.
    let mut event = LogEvent::from("flat");
    *event.metadata_mut().value_mut() = build_alternating(39);
    let array = EventArray::Logs(LogArray::from(vec![event]));
    let mut buf = BytesMut::with_capacity(65536);
    assert!(
        matches!(
            array.encode(&mut buf),
            Err(super::super::ser::EncodeError::NestingTooDeep { .. })
        ),
        "nesting gate should reject 39 alternating metadata levels (cost 97)",
    );
}

/// Verify the gate rejects a `Value::Timestamp` leaf sitting at the deepest object
/// position the budget would otherwise allow, and that the underlying proto roundtrip
/// would in fact fail there — confirming the timestamp leaf is not free.
#[test]
fn nesting_gate_rejects_timestamp_leaf_at_max_object_depth() {
    let roundtrip_log = |value: Value| -> bool {
        let mut event = LogEvent::default();
        event.insert(event_path!("data"), value);
        let array = EventArray::Logs(LogArray::from(vec![event]));
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        proto::EventArray::decode(buf.freeze()).is_ok()
    };
    let roundtrip_metadata = |value: Value| -> bool {
        let mut event = LogEvent::from("flat");
        *event.metadata_mut().value_mut() = value;
        let array = EventArray::Logs(LogArray::from(vec![event]));
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        proto::EventArray::decode(buf.freeze()).is_ok()
    };

    // `Log.fields` can carry 33 object levels (cost 99), but a Timestamp leaf raises
    // that cost to 100 and fails decode. The gate rejects it under the common limit too.
    let event_data_ts = create_nested_value_with_leaf(MAX_OBJECT_DEPTH_VALUE, ts_leaf());
    assert!(
        !roundtrip_log(event_data_ts.clone()),
        "depth {} with Timestamp leaf is expected to fail prost decode",
        MAX_OBJECT_DEPTH_VALUE + 1,
    );

    let mut event = LogEvent::default();
    event.insert(event_path!("data"), event_data_ts);
    let array = EventArray::Logs(LogArray::from(vec![event]));
    let mut buf = BytesMut::with_capacity(65536);
    assert!(
        matches!(
            array.encode(&mut buf),
            Err(super::super::ser::EncodeError::NestingTooDeep { .. })
        ),
        "gate should reject event-data Timestamp leaf above the common budget",
    );

    // Metadata reaches its wire boundary at the common 32-object limit.
    let metadata_ts = create_nested_value_with_leaf(MAX_OBJECT_DEPTH_VALUE, ts_leaf());
    assert!(
        !roundtrip_metadata(metadata_ts.clone()),
        "metadata depth {MAX_OBJECT_DEPTH_VALUE} with Timestamp leaf is expected to fail prost decode"
    );

    let mut event = LogEvent::from("flat");
    *event.metadata_mut().value_mut() = metadata_ts;
    let array = EventArray::Logs(LogArray::from(vec![event]));
    let mut buf = BytesMut::with_capacity(65536);
    assert!(
        matches!(
            array.encode(&mut buf),
            Err(super::super::ser::EncodeError::NestingTooDeep { .. })
        ),
        "gate should reject metadata Timestamp leaf at object depth {MAX_OBJECT_DEPTH_VALUE}",
    );
}

/// Verify the gate still admits Timestamp leaves one level shallower than the boundary
/// — they cost exactly one frame, no more — and that those payloads roundtrip cleanly
/// through prost.
#[test]
fn nesting_gate_accepts_timestamp_leaf_below_max_object_depth() {
    // One object level below the common limit plus a Timestamp leaf.
    let mut event = LogEvent::default();
    event.insert(
        event_path!("data"),
        create_nested_value_with_leaf(MAX_OBJECT_DEPTH_VALUE - 2, ts_leaf()),
    );
    let array = EventArray::Logs(LogArray::from(vec![event]));
    let mut buf = BytesMut::with_capacity(65536);
    assert!(
        array.encode(&mut buf).is_ok(),
        "gate should accept event-data Timestamp leaf at object depth {}",
        MAX_OBJECT_DEPTH_VALUE - 1,
    );
    assert!(
        proto::EventArray::decode(buf.freeze()).is_ok(),
        "prost should decode event-data Timestamp leaf at object depth {}",
        MAX_OBJECT_DEPTH_VALUE - 1,
    );

    // Metadata: one shallower.
    let mut event = LogEvent::from("flat");
    *event.metadata_mut().value_mut() =
        create_nested_value_with_leaf(MAX_OBJECT_DEPTH_VALUE - 1, ts_leaf());
    let array = EventArray::Logs(LogArray::from(vec![event]));
    let mut buf = BytesMut::with_capacity(65536);
    assert!(
        array.encode(&mut buf).is_ok(),
        "gate should accept metadata Timestamp leaf at object depth {}",
        MAX_OBJECT_DEPTH_VALUE - 1,
    );
    assert!(
        proto::EventArray::decode(buf.freeze()).is_ok(),
        "prost should decode metadata Timestamp leaf at object depth {}",
        MAX_OBJECT_DEPTH_VALUE - 1,
    );
}

/// Verify `filter_unencodable` keeps the valid events and drops only the over-budget
/// ones, returning a smaller `EventArray` rather than failing the whole batch.
#[test]
fn filter_unencodable_drops_only_over_budget_events() {
    let good = || {
        let mut event = LogEvent::default();
        event.insert(event_path!("data"), "ok");
        event
    };
    let bad = || {
        let mut event = LogEvent::default();
        event.insert(
            event_path!("data"),
            create_nested_value(MAX_OBJECT_DEPTH_VALUE),
        );
        event
    };

    let logs = vec![good(), bad(), good(), bad(), good()];
    let array = EventArray::Logs(LogArray::from(logs));

    let filtered = array
        .filter_unencodable()
        .expect("3 good events should survive filtering");
    assert_eq!(filtered.event_count(), 3, "only good events should remain");

    let EventArray::Logs(surviving) = filtered else {
        panic!("variant should be preserved");
    };
    for log in &surviving {
        assert_eq!(
            log.value()
                .get(vrl::path!("data"))
                .and_then(|v| v.as_bytes()),
            Some(&bytes::Bytes::from_static(b"ok")),
            "only good events should remain",
        );
    }
}

/// Verify that the public per-event entry point used by both the native codec and the
/// vector sink charges `Value::Timestamp` for one frame, just like the buffer gate.
/// Without this, a deep object chain ending in a timestamp could pass the codec check
/// and fail prost decode on the receiving end.
#[test]
fn event_exceeds_max_nesting_cost_charges_timestamp_leaf() {
    let log_at_max_with_ts = {
        let mut event = LogEvent::default();
        event.insert(
            event_path!("data"),
            create_nested_value_with_leaf(MAX_OBJECT_DEPTH_VALUE - 1, ts_leaf()),
        );
        Event::Log(event)
    };
    assert!(
        event_exceeds_max_nesting_cost(&log_at_max_with_ts).is_some(),
        "depth {MAX_OBJECT_DEPTH_VALUE} log with Timestamp leaf must be rejected",
    );

    let trace_at_max_with_ts = {
        let mut trace = TraceEvent::default();
        trace.insert(
            event_path!("data"),
            create_nested_value_with_leaf(MAX_OBJECT_DEPTH_VALUE - 1, ts_leaf()),
        );
        Event::Trace(trace)
    };
    assert!(
        event_exceeds_max_nesting_cost(&trace_at_max_with_ts).is_some(),
        "depth {MAX_OBJECT_DEPTH_VALUE} trace with Timestamp leaf must be rejected",
    );

    let metric_at_max_with_ts = {
        let mut metric = Metric::new(
            "test",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        );
        *metric.metadata_mut().value_mut() =
            create_nested_value_with_leaf(MAX_OBJECT_DEPTH_VALUE, ts_leaf());
        Event::Metric(metric)
    };
    assert!(
        event_exceeds_max_nesting_cost(&metric_at_max_with_ts).is_some(),
        "metric with metadata-Timestamp leaf at depth {MAX_OBJECT_DEPTH_VALUE} must be rejected",
    );

    // And one shallower stays under the budget.
    let log_below_max_with_ts = {
        let mut event = LogEvent::default();
        event.insert(
            event_path!("data"),
            create_nested_value_with_leaf(MAX_OBJECT_DEPTH_VALUE - 2, ts_leaf()),
        );
        Event::Log(event)
    };
    assert!(
        event_exceeds_max_nesting_cost(&log_below_max_with_ts).is_none(),
        "depth {} log with Timestamp leaf must be accepted",
        MAX_OBJECT_DEPTH_VALUE - 1,
    );
}

/// Unit-level check that `check_value_nesting_cost` charges `TIMESTAMP_FRAME_COST`
/// for a `Value::Timestamp` leaf, independent of nesting.
#[test]
fn check_value_nesting_cost_charges_timestamp_leaf() {
    let ts = ts_leaf();
    assert!(check_value_nesting_cost(&ts, 0, TIMESTAMP_FRAME_COST).is_ok());
    assert!(check_value_nesting_cost(&ts, 0, TIMESTAMP_FRAME_COST - 1).is_err());

    // A single object level containing a timestamp leaf: OBJECT_FRAME_COST + TIMESTAMP_FRAME_COST.
    let mut map = ObjectMap::new();
    map.insert("ts".into(), ts);
    let nested = Value::Object(map);
    let expected = OBJECT_FRAME_COST + TIMESTAMP_FRAME_COST;
    assert!(check_value_nesting_cost(&nested, 0, expected).is_ok());
    assert!(check_value_nesting_cost(&nested, 0, expected - 1).is_err());
}

/// Verify flat events pass without issues.
#[test]
fn nesting_gate_accepts_flat_events() {
    let mut log = LogEvent::from("hello world");
    log.insert(event_path!("foo"), "bar");
    let events = EventArray::Logs(LogArray::from(vec![log]));
    let mut buf = BytesMut::with_capacity(1024);
    assert!(events.encode(&mut buf).is_ok());

    let mut trace = TraceEvent::default();
    trace.insert(event_path!("foo"), "bar");
    let events = EventArray::Traces(TraceArray::from(vec![trace]));
    let mut buf = BytesMut::with_capacity(1024);
    assert!(events.encode(&mut buf).is_ok());

    let metric = Metric::new(
        "test_counter",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    );
    let events = EventArray::Metrics(MetricArray::from(vec![metric]));
    let mut buf = BytesMut::with_capacity(1024);
    assert!(events.encode(&mut buf).is_ok());
}

#[test]
fn check_value_nesting_cost_with_configurable_budget() {
    // Five nested objects: 5 levels × 3 frames per object = 15 frame cost.
    let mut value = Value::from("leaf");
    for _ in 0..5 {
        let mut map = ObjectMap::new();
        map.insert("n".into(), value);
        value = Value::Object(map);
    }

    assert!(check_value_nesting_cost(&value, 0, 15).is_ok());
    assert!(check_value_nesting_cost(&value, 0, 14).is_err());
    assert!(check_value_nesting_cost(&value, 0, 30).is_ok());

    let flat = Value::from("hello");
    assert!(check_value_nesting_cost(&flat, 0, 0).is_ok());
}

#[test]
fn check_value_nesting_cost_with_mixed_variants() {
    // Outer array (2) → inner object (3) → inner array (2) → leaf = 7 frame cost.
    let inner = Value::Array(vec![Value::from("leaf")]);
    let mut map = ObjectMap::new();
    map.insert("arr".into(), inner);
    let value = Value::Array(vec![Value::Object(map)]);

    assert!(check_value_nesting_cost(&value, 0, 7).is_ok());
    assert!(check_value_nesting_cost(&value, 0, 6).is_err());
}
