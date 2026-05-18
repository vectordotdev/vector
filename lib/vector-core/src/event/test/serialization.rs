use super::*;
use crate::config::log_schema;
use bytes::{Buf, BufMut, BytesMut};
use prost::Message;
use quickcheck::{QuickCheck, TestResult};
use regex::Regex;
use similar_asserts::assert_eq;
use vector_buffers::encoding::Encodable;

use crate::event::ser::{
    ARRAY_FRAME_COST, MAX_METADATA_VALUE_NESTING_FRAMES, MAX_VALUE_NESTING_FRAMES,
    OBJECT_FRAME_COST, check_value_nesting_cost,
};

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
    event.insert("foo", "bar");
    event.insert("bar", "baz");

    let expected_all = serde_json::json!({
        "message": "raw log line",
        "foo": "bar",
        "bar": "baz",
        "timestamp": event.get(log_schema().timestamp_key().unwrap().to_string().as_str()),
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
    event.insert("int", 4);
    event.insert("float", 5.5);
    event.insert("bool", true);
    event.insert("string", "thisisastring");

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
// Each encoding path has a fixed proto-wrapper overhead before the Value tree starts:
//
//   - Event data path (Log.fields, Trace.fields):  frame budget MAX_VALUE_NESTING_FRAMES (99)
//   - Metadata path (metadata_full):               frame budget MAX_METADATA_VALUE_NESTING_FRAMES (96)
//
// The `per_path_boundaries` test verifies both budgets empirically via prost roundtrip.
//
// The saturated-event tests create events with ALL Value-carrying fields at their
// respective max frame cost simultaneously. The proto conversion code populates every
// field (including deprecated ones like Log.metadata), so a single roundtrip per event
// type covers every proto path automatically.

/// Maximum number of object-only nesting levels that fit the event-data frame budget.
const MAX_OBJECT_DEPTH_VALUE: usize = MAX_VALUE_NESTING_FRAMES / OBJECT_FRAME_COST;

/// Maximum number of object-only nesting levels that fit the metadata frame budget.
const MAX_OBJECT_DEPTH_METADATA: usize = MAX_METADATA_VALUE_NESTING_FRAMES / OBJECT_FRAME_COST;

/// Maximum number of array-only nesting levels that fit the event-data frame budget.
const MAX_ARRAY_DEPTH_VALUE: usize = MAX_VALUE_NESTING_FRAMES / ARRAY_FRAME_COST;

/// Maximum number of array-only nesting levels that fit the metadata frame budget.
const MAX_ARRAY_DEPTH_METADATA: usize = MAX_METADATA_VALUE_NESTING_FRAMES / ARRAY_FRAME_COST;

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

/// Create a [`LogEvent`] with event data at `value_depth` and metadata at `metadata_depth`.
fn create_saturated_log(value_depth: usize, metadata_depth: usize) -> LogEvent {
    let mut event = LogEvent::default();
    event.insert("data", create_nested_value(value_depth - 1));
    *event.metadata_mut().value_mut() = create_nested_value(metadata_depth);
    event
}

/// Create a [`TraceEvent`] with event data at `value_depth` and metadata at `metadata_depth`.
fn create_saturated_trace(value_depth: usize, metadata_depth: usize) -> TraceEvent {
    let mut trace = TraceEvent::default();
    trace.insert("data", create_nested_value(value_depth - 1));
    *trace.metadata_mut().value_mut() = create_nested_value(metadata_depth);
    trace
}

/// Create a Metric with metadata at `metadata_depth`.
/// (Metric values have fixed structure — only metadata carries arbitrary Values.)
fn create_saturated_metric(metadata_depth: usize) -> Metric {
    let mut metric = Metric::new(
        "test",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    );
    *metric.metadata_mut().value_mut() = create_nested_value(metadata_depth);
    metric
}

/// Build all three `EventArray` variants with each field at its respective max depth.
fn saturated_event_arrays(
    value_depth: usize,
    metadata_depth: usize,
) -> Vec<(&'static str, EventArray)> {
    vec![
        (
            "Log",
            EventArray::Logs(LogArray::from(vec![create_saturated_log(
                value_depth,
                metadata_depth,
            )])),
        ),
        (
            "Trace",
            EventArray::Traces(TraceArray::from(vec![create_saturated_trace(
                value_depth,
                metadata_depth,
            )])),
        ),
        (
            "Metric",
            EventArray::Metrics(MetricArray::from(vec![create_saturated_metric(
                metadata_depth,
            )])),
        ),
    ]
}

/// Build all three Event variants for `EventWrapper` encoding.
fn saturated_events(value_depth: usize, metadata_depth: usize) -> Vec<(&'static str, Event)> {
    vec![
        (
            "Log",
            Event::Log(create_saturated_log(value_depth, metadata_depth)),
        ),
        (
            "Trace",
            Event::Trace(create_saturated_trace(value_depth, metadata_depth)),
        ),
        (
            "Metric",
            Event::Metric(create_saturated_metric(metadata_depth)),
        ),
    ]
}

/// Verify the frame budgets are exactly right: all event types roundtrip at the
/// max object-only depth, and at least one fails prost decode when either budget
/// is exceeded.
#[test]
fn max_nesting_budgets_are_correct() {
    let max_val = MAX_OBJECT_DEPTH_VALUE;
    let max_meta = MAX_OBJECT_DEPTH_METADATA;

    // --- Both budgets at max must roundtrip for all event types ---

    for (name, array) in saturated_event_arrays(max_val, max_meta) {
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        assert!(
            proto::EventArray::decode(buf.freeze()).is_ok(),
            "EventArray decode FAILED for {name} at value depth {max_val}, metadata depth {max_meta}.",
        );
    }

    for (name, event) in saturated_events(max_val, max_meta) {
        let wrapper = proto::EventWrapper::from(event);
        let mut buf = BytesMut::with_capacity(65536);
        wrapper.encode(&mut buf).unwrap();
        assert!(
            proto::EventWrapper::decode(buf.freeze()).is_ok(),
            "EventWrapper decode FAILED for {name} at value depth {max_val}, metadata depth {max_meta}.",
        );
    }

    // --- Exceeding either budget must fail for at least one event type ---

    // Exceed value budget
    let any_fails = saturated_event_arrays(max_val + 1, max_meta)
        .into_iter()
        .any(|(_, array)| {
            let proto_array = proto::EventArray::from(array);
            let mut buf = BytesMut::with_capacity(65536);
            proto_array.encode(&mut buf).unwrap();
            proto::EventArray::decode(buf.freeze()).is_err()
        });
    assert!(
        any_fails,
        "No path failed at object value depth {}. MAX_VALUE_NESTING_FRAMES could be raised.",
        max_val + 1
    );

    // Exceed metadata budget
    let any_fails = saturated_event_arrays(max_val, max_meta + 1)
        .into_iter()
        .any(|(_, array)| {
            let proto_array = proto::EventArray::from(array);
            let mut buf = BytesMut::with_capacity(65536);
            proto_array.encode(&mut buf).unwrap();
            proto::EventArray::decode(buf.freeze()).is_err()
        });
    assert!(
        any_fails,
        "No path failed at object metadata depth {}. MAX_METADATA_VALUE_NESTING_FRAMES could be raised.",
        max_meta + 1
    );
}

/// Verify the nesting gate accepts all event types at the max object-only depth.
#[test]
fn nesting_gate_accepts_all_types_at_max_depth() {
    for (name, array) in saturated_event_arrays(MAX_OBJECT_DEPTH_VALUE, MAX_OBJECT_DEPTH_METADATA) {
        let mut buf = BytesMut::with_capacity(65536);
        assert!(
            array.encode(&mut buf).is_ok(),
            "nesting gate rejected {name} at max object depths",
        );
    }
}

/// Verify the nesting gate rejects when either object-only budget is exceeded.
#[test]
fn nesting_gate_rejects_above_max_depth() {
    // Exceed value budget (Log and Trace have event data; Metric does not)
    for (name, array) in
        saturated_event_arrays(MAX_OBJECT_DEPTH_VALUE + 1, MAX_OBJECT_DEPTH_METADATA)
    {
        // Metric has no event data field, so it won't be rejected here
        if name == "Metric" {
            continue;
        }
        let mut buf = BytesMut::with_capacity(65536);
        assert!(
            matches!(
                array.encode(&mut buf),
                Err(super::super::ser::EncodeError::NestingTooDeep { .. })
            ),
            "nesting gate should reject {name} at object value depth {}",
            MAX_OBJECT_DEPTH_VALUE + 1,
        );
    }

    // Exceed metadata budget
    for (name, array) in
        saturated_event_arrays(MAX_OBJECT_DEPTH_VALUE, MAX_OBJECT_DEPTH_METADATA + 1)
    {
        let mut buf = BytesMut::with_capacity(65536);
        assert!(
            matches!(
                array.encode(&mut buf),
                Err(super::super::ser::EncodeError::NestingTooDeep { .. })
            ),
            "nesting gate should reject {name} at object metadata depth {}",
            MAX_OBJECT_DEPTH_METADATA + 1,
        );
    }
}

/// Verify the per-path prost boundaries match the budgets for both object-only and
/// array-only nesting.
///
/// Object-only `Log.fields`:     depth 33 succeeds, 34 fails.
/// Object-only `metadata_full`:  depth 32 succeeds, 33 fails.
/// Array-only  `Log.fields`:     depth 49 succeeds, 50 fails.
/// Array-only  `metadata_full`:  depth 48 succeeds, 49 fails.
#[test]
fn per_path_boundaries() {
    let roundtrip_value = |value: Value| -> bool {
        let mut event = LogEvent::default();
        event.insert("data", value);
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

    // Object-only Log.fields: the "data" key contributes one level on top of the inner
    // nested value, so we subtract one when building the value.
    assert!(
        roundtrip_value(create_nested_value(MAX_OBJECT_DEPTH_VALUE - 1)),
        "Log.fields should succeed at object depth {MAX_OBJECT_DEPTH_VALUE}"
    );
    assert!(
        !roundtrip_value(create_nested_value(MAX_OBJECT_DEPTH_VALUE)),
        "Log.fields should fail at object depth {}",
        MAX_OBJECT_DEPTH_VALUE + 1
    );

    // Object-only metadata_full: metadata Value is the root, no key on top.
    assert!(
        roundtrip_metadata(create_nested_value(MAX_OBJECT_DEPTH_METADATA)),
        "metadata_full should succeed at object depth {MAX_OBJECT_DEPTH_METADATA}"
    );
    assert!(
        !roundtrip_metadata(create_nested_value(MAX_OBJECT_DEPTH_METADATA + 1)),
        "metadata_full should fail at object depth {}",
        MAX_OBJECT_DEPTH_METADATA + 1
    );

    // Array-only Log.fields: array contributes 2 frames per level, so it fits more levels.
    assert!(
        roundtrip_value(create_nested_array(MAX_ARRAY_DEPTH_VALUE - 1)),
        "Log.fields should succeed at array depth {MAX_ARRAY_DEPTH_VALUE}"
    );
    assert!(
        !roundtrip_value(create_nested_array(MAX_ARRAY_DEPTH_VALUE)),
        "Log.fields should fail at array depth {}",
        MAX_ARRAY_DEPTH_VALUE + 1
    );

    // Array-only metadata_full
    assert!(
        roundtrip_metadata(create_nested_array(MAX_ARRAY_DEPTH_METADATA)),
        "metadata_full should succeed at array depth {MAX_ARRAY_DEPTH_METADATA}"
    );
    assert!(
        !roundtrip_metadata(create_nested_array(MAX_ARRAY_DEPTH_METADATA + 1)),
        "metadata_full should fail at array depth {}",
        MAX_ARRAY_DEPTH_METADATA + 1
    );
}

/// Verify that array-only nesting deeper than the object-only cap (33) is accepted by
/// the gate — this is the regression that the frame-cost check addresses. Previously a
/// uniform depth-33 cap dropped array-only events that prost would happily roundtrip.
#[test]
fn nesting_gate_accepts_deep_array_nesting() {
    // An array depth 40 = 80 frames, comfortably under the 99-frame value budget but well
    // over the 33-depth limit the old uniform check would have applied.
    let mut event = LogEvent::default();
    event.insert("data", create_nested_array(40));
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
    // Under the metadata budget of 96. Fits.
    let mut event = LogEvent::from("flat");
    *event.metadata_mut().value_mut() = build_alternating(38);
    let array = EventArray::Logs(LogArray::from(vec![event]));
    let mut buf = BytesMut::with_capacity(65536);
    assert!(
        array.encode(&mut buf).is_ok(),
        "nesting gate should accept 38 alternating metadata levels (cost 95)",
    );

    // 39 alternating levels: 20 array (cost 40) + 19 object (cost 57) = 97 frames.
    // Over the metadata budget of 96. Fails.
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

/// Verify flat events pass without issues.
#[test]
fn nesting_gate_accepts_flat_events() {
    let mut log = LogEvent::from("hello world");
    log.insert("foo", "bar");
    let events = EventArray::Logs(LogArray::from(vec![log]));
    let mut buf = BytesMut::with_capacity(1024);
    assert!(events.encode(&mut buf).is_ok());

    let mut trace = TraceEvent::default();
    trace.insert("foo", "bar");
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
