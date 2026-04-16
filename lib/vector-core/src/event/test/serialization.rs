use bytes::{Buf, BufMut, BytesMut};
use prost::Message;
use quickcheck::{QuickCheck, TestResult};
use regex::Regex;
use similar_asserts::assert_eq;
use vector_buffers::encoding::Encodable;

use super::*;
use crate::config::log_schema;
use crate::event::ser::check_value_depth;

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
// Nesting depth validation tests
// ---------------------------------------------------------------------------
//
// MAX_NESTING_DEPTH (32) is the highest Value depth that roundtrips through prost
// encode/decode across ALL proto encoding paths. The limit is constrained by prost's
// fixed recursion budget of 100 (RECURSION_LIMIT in prost/src/lib.rs).
//
// Each Value nesting level consumes 3 prost recursion entries (Value + ValueMap +
// map_entry for Objects), and each encoding path has a different number of proto
// wrapper messages before the Value tree starts. The tightest path is metadata_full
// (EventArray → *Array → Event → Metadata → Value) which uses exactly 100/100
// budget at depth 32.
//
// Rather than enumerating individual proto paths, the tests below create events with
// ALL Value-carrying fields set to max depth simultaneously. The proto conversion code
// populates every field (including deprecated ones like Log.metadata), so a single
// roundtrip covers every path for a given event type. If a new Value-carrying field
// is added to the proto schema, the conversion code must populate it, and these tests
// automatically cover it with zero maintenance.

/// Creates a Value with the specified number of nested Object wrapping levels.
///
/// Returns a Value that is `wrapping_levels` nested Objects deep, with a string leaf.
/// `check_value_depth` will measure this as depth `wrapping_levels` (the leaf).
fn create_nested_value(wrapping_levels: usize) -> Value {
    let mut value = Value::from("innermost");
    for _ in 0..wrapping_levels {
        let mut map = ObjectMap::new();
        map.insert("nested".into(), value);
        value = Value::Object(map);
    }
    value
}

/// Create a [`LogEvent`] with both event data and metadata at the given nesting depth.
fn create_saturated_log(depth: usize) -> LogEvent {
    let nested = create_nested_value(depth);
    let mut event = LogEvent::default();
    // Event data: "data" key + (depth-1) wrapping levels = leaf at `depth`.
    // This covers Log.fields (or Log.value for non-Object roots).
    event.insert("data", create_nested_value(depth - 1));
    // Metadata: covers both Log.metadata (deprecated) and Log.metadata_full.value.
    *event.metadata_mut().value_mut() = nested;
    event
}

/// Create a [`TraceEvent`] with both event data and metadata at the given nesting depth.
fn create_saturated_trace(depth: usize) -> TraceEvent {
    let nested = create_nested_value(depth);
    let mut trace = TraceEvent::default();
    trace.insert("data", create_nested_value(depth - 1));
    *trace.metadata_mut().value_mut() = nested;
    trace
}

/// Create a Metric with metadata at the given nesting depth.
/// (Metric values have fixed structure — only metadata carries arbitrary Values.)
fn create_saturated_metric(depth: usize) -> Metric {
    let mut metric = Metric::new(
        "test",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    );
    *metric.metadata_mut().value_mut() = create_nested_value(depth);
    metric
}

/// Build all three `EventArray` variants with every Value field at the given depth.
fn saturated_event_arrays(depth: usize) -> Vec<(&'static str, EventArray)> {
    vec![
        (
            "Log",
            EventArray::Logs(LogArray::from(vec![create_saturated_log(depth)])),
        ),
        (
            "Trace",
            EventArray::Traces(TraceArray::from(vec![create_saturated_trace(depth)])),
        ),
        (
            "Metric",
            EventArray::Metrics(MetricArray::from(vec![create_saturated_metric(depth)])),
        ),
    ]
}

/// Build all three Event variants for `EventWrapper` encoding.
fn saturated_events(depth: usize) -> Vec<(&'static str, Event)> {
    vec![
        ("Log", Event::Log(create_saturated_log(depth))),
        ("Trace", Event::Trace(create_saturated_trace(depth))),
        ("Metric", Event::Metric(create_saturated_metric(depth))),
    ]
}

/// Verify `MAX_NESTING_DEPTH` is exactly right: all event types roundtrip at depth 32,
/// and at least one fails prost decode at depth 33.
///
/// Each event has ALL Value-carrying fields saturated at the test depth, so every
/// proto field (including deprecated ones) is exercised. No path enumeration needed —
/// if a new Value field is added to the proto schema, the conversion code populates it
/// and this test covers it automatically.
#[test]
fn max_nesting_depth_is_correct() {
    let max = super::super::ser::MAX_NESTING_DEPTH;

    // --- Depth MAX must roundtrip for all event types ---

    for (name, array) in saturated_event_arrays(max) {
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        assert!(
            proto::EventArray::decode(buf.freeze()).is_ok(),
            "EventArray decode FAILED at depth {max} for {name}.\n\
             MAX_NESTING_DEPTH is too high — lower it or check for new proto wrappers.",
        );
    }

    for (name, event) in saturated_events(max) {
        let wrapper = proto::EventWrapper::from(event);
        let mut buf = BytesMut::with_capacity(65536);
        wrapper.encode(&mut buf).unwrap();
        assert!(
            proto::EventWrapper::decode(buf.freeze()).is_ok(),
            "EventWrapper decode FAILED at depth {max} for {name}.\n\
             MAX_NESTING_DEPTH is too high for the EventWrapper path.",
        );
    }

    // --- Depth MAX+1 must fail for at least one event type ---
    // (proves the limit can't be raised without hitting prost's recursion limit)

    let any_array_fails = saturated_event_arrays(max + 1)
        .into_iter()
        .any(|(_, array)| {
            let proto_array = proto::EventArray::from(array);
            let mut buf = BytesMut::with_capacity(65536);
            proto_array.encode(&mut buf).unwrap();
            proto::EventArray::decode(buf.freeze()).is_err()
        });
    assert!(
        any_array_fails,
        "All EventArray types decoded at depth {}. MAX_NESTING_DEPTH ({}) could be raised.",
        max + 1,
        max,
    );

    let any_wrapper_fails = saturated_events(max + 1).into_iter().any(|(_, event)| {
        let wrapper = proto::EventWrapper::from(event);
        let mut buf = BytesMut::with_capacity(65536);
        wrapper.encode(&mut buf).unwrap();
        proto::EventWrapper::decode(buf.freeze()).is_err()
    });
    assert!(
        any_wrapper_fails,
        "All EventWrapper types decoded at depth {}. MAX_NESTING_DEPTH ({}) could be raised.",
        max + 1,
        max,
    );
}

/// Verify the nesting gate accepts all event types at `MAX_NESTING_DEPTH`.
#[test]
fn nesting_gate_accepts_all_types_at_max_depth() {
    for (name, array) in saturated_event_arrays(super::super::ser::MAX_NESTING_DEPTH) {
        let mut buf = BytesMut::with_capacity(65536);
        assert!(
            array.encode(&mut buf).is_ok(),
            "nesting gate rejected {name} at MAX_NESTING_DEPTH",
        );
    }
}

/// Verify the nesting gate rejects all event types at `MAX_NESTING_DEPTH` + 1.
#[test]
fn nesting_gate_rejects_all_types_above_max_depth() {
    let max = super::super::ser::MAX_NESTING_DEPTH;
    for (name, array) in saturated_event_arrays(max + 1) {
        let mut buf = BytesMut::with_capacity(65536);
        assert!(
            matches!(
                array.encode(&mut buf),
                Err(super::super::ser::EncodeError::NestingTooDeep { .. })
            ),
            "nesting gate should reject {name} at depth {}",
            max + 1,
        );
    }
}

/// Verify the per-path boundaries for the loosest and tightest encoding paths.
///
/// `Log.fields` is the loosest path (3 entries of headroom at depth 32):
///   - depth 33 succeeds (100/100 budget)
///   - depth 34 fails (103/100 budget)
///
/// `metadata_full` is the tightest path (0 headroom at depth 32):
///   - depth 32 succeeds (100/100 budget)
///   - depth 33 fails (103/100 budget)
///
/// The uniform `MAX_NESTING_DEPTH = 32` is set by the tightest path.
#[test]
fn per_path_boundaries() {
    let max = super::super::ser::MAX_NESTING_DEPTH;

    // Helper: encode a LogEvent with nested value and flat metadata via EventArray.
    let roundtrip_value = |depth: usize| -> bool {
        let mut event = LogEvent::default();
        event.insert("data", create_nested_value(depth - 1));
        let array = EventArray::Logs(LogArray::from(vec![event]));
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        proto::EventArray::decode(buf.freeze()).is_ok()
    };

    // Helper: encode a LogEvent with flat value and nested metadata via EventArray.
    let roundtrip_metadata = |depth: usize| -> bool {
        let mut event = LogEvent::from("flat");
        *event.metadata_mut().value_mut() = create_nested_value(depth);
        let array = EventArray::Logs(LogArray::from(vec![event]));
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        proto::EventArray::decode(buf.freeze()).is_ok()
    };

    // Log.fields (loosest): succeeds at 33, fails at 34
    assert!(
        roundtrip_value(max + 1),
        "Log.fields should succeed at depth {}",
        max + 1
    );
    assert!(
        !roundtrip_value(max + 2),
        "Log.fields should fail at depth {}",
        max + 2
    );

    // metadata_full (tightest): succeeds at 32, fails at 33
    assert!(
        roundtrip_metadata(max),
        "metadata_full should succeed at depth {max}"
    );
    assert!(
        !roundtrip_metadata(max + 1),
        "metadata_full should fail at depth {}",
        max + 1
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
fn check_value_depth_with_configurable_limit() {
    let mut value = Value::from("leaf");
    for _ in 0..5 {
        let mut map = ObjectMap::new();
        map.insert("n".into(), value);
        value = Value::Object(map);
    }

    assert!(check_value_depth(&value, 0, 5).is_ok());
    assert!(check_value_depth(&value, 0, 4).is_err());
    assert!(check_value_depth(&value, 0, 10).is_ok());

    let flat = Value::from("hello");
    assert!(check_value_depth(&flat, 0, 0).is_ok());
}

#[test]
fn check_value_depth_with_arrays() {
    // Array containing an object containing an array containing a value = depth 3
    let inner = Value::Array(vec![Value::from("leaf")]);
    let mut map = ObjectMap::new();
    map.insert("arr".into(), inner);
    let value = Value::Array(vec![Value::Object(map)]);

    assert!(check_value_depth(&value, 0, 3).is_ok());
    assert!(check_value_depth(&value, 0, 2).is_err());
}
