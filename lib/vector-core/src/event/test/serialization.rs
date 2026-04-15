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
// The exhaustive test below (`max_nesting_depth_is_correct_for_all_proto_paths`)
// verifies every encoding path at both boundaries: depth 32 must roundtrip, depth 33
// must fail prost decode. If a proto schema change adds wrapper messages, or prost
// changes its recursion limit, this test will fail and tell you exactly which path
// broke.

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

/// Create a `LogEvent` with the specified nesting depth of maps.
///
/// The resulting `LogEvent` has a root Object (depth 0) containing a "data" key
/// whose value is `wrapping_levels` levels of nested maps with a string leaf.
/// The leaf string is at effective depth `wrapping_levels + 1` from the root.
fn create_nested_log_event(wrapping_levels: usize) -> LogEvent {
    let mut event = LogEvent::default();
    event.insert("data", create_nested_value(wrapping_levels));
    event
}

/// Describes a proto encoding path for testing.
struct ProtoPath {
    /// Human-readable name for error messages.
    name: &'static str,
    /// Creates an EventArray with a Value at the given `check_value_depth` depth.
    make_event_array: fn(depth: usize) -> EventArray,
    /// Creates an EventWrapper with a Value at the given depth (None if path is
    /// EventArray-only and has no EventWrapper equivalent).
    make_event_wrapper: Option<fn(depth: usize) -> Event>,
}

/// Returns all proto encoding paths that carry an arbitrary `Value`.
///
/// Each path represents a distinct route through the proto schema where a deeply
/// nested Value could hit prost's recursion limit. When adding new proto message
/// wrappers or new Value-carrying fields, add the corresponding path here.
fn all_proto_paths() -> Vec<ProtoPath> {
    vec![
        // --- Log event data (Object root → Log.fields) ---
        ProtoPath {
            name: "EventArray → LogArray → Log → Log.fields",
            make_event_array: |depth| {
                // depth-1 wrapping levels + "data" key = leaf at `depth`
                let event = create_nested_log_event(depth - 1);
                EventArray::Logs(LogArray::from(vec![event]))
            },
            make_event_wrapper: Some(|depth| {
                Event::Log(create_nested_log_event(depth - 1))
            }),
        },
        // --- Log metadata (via metadata_full → Metadata → Value) ---
        ProtoPath {
            name: "EventArray → LogArray → Log → Metadata → Value (metadata_full)",
            make_event_array: |depth| {
                let mut event = LogEvent::from("data");
                *event.metadata_mut().value_mut() = create_nested_value(depth);
                EventArray::Logs(LogArray::from(vec![event]))
            },
            make_event_wrapper: Some(|depth| {
                let mut event = LogEvent::from("data");
                *event.metadata_mut().value_mut() = create_nested_value(depth);
                Event::Log(event)
            }),
        },
        // --- Trace event data (Trace.fields) ---
        ProtoPath {
            name: "EventArray → TraceArray → Trace → Trace.fields",
            make_event_array: |depth| {
                let mut trace = TraceEvent::default();
                trace.insert("data", create_nested_value(depth - 1));
                EventArray::Traces(TraceArray::from(vec![trace]))
            },
            make_event_wrapper: Some(|depth| {
                let mut trace = TraceEvent::default();
                trace.insert("data", create_nested_value(depth - 1));
                Event::Trace(trace)
            }),
        },
        // --- Trace metadata (via metadata_full) ---
        ProtoPath {
            name: "EventArray → TraceArray → Trace → Metadata → Value (metadata_full)",
            make_event_array: |depth| {
                let mut trace = TraceEvent::default();
                trace.insert("placeholder", "data");
                *trace.metadata_mut().value_mut() = create_nested_value(depth);
                EventArray::Traces(TraceArray::from(vec![trace]))
            },
            make_event_wrapper: Some(|depth| {
                let mut trace = TraceEvent::default();
                trace.insert("placeholder", "data");
                *trace.metadata_mut().value_mut() = create_nested_value(depth);
                Event::Trace(trace)
            }),
        },
        // --- Metric metadata (via metadata_full) ---
        ProtoPath {
            name: "EventArray → MetricArray → Metric → Metadata → Value (metadata_full)",
            make_event_array: |depth| {
                let mut metric = Metric::new(
                    "test",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                );
                *metric.metadata_mut().value_mut() = create_nested_value(depth);
                EventArray::Metrics(MetricArray::from(vec![metric]))
            },
            make_event_wrapper: Some(|depth| {
                let mut metric = Metric::new(
                    "test",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                );
                *metric.metadata_mut().value_mut() = create_nested_value(depth);
                Event::Metric(metric)
            }),
        },
    ]
}

/// Helper: encode an EventArray to proto bytes, bypassing the nesting gate.
fn proto_encode_event_array(array: EventArray) -> BytesMut {
    let proto_array = proto::EventArray::from(array);
    let mut buffer = BytesMut::with_capacity(65536);
    proto_array.encode(&mut buffer).expect("prost encode should always succeed (no encode-side recursion limit)");
    buffer
}

/// Helper: encode an Event as an EventWrapper to proto bytes.
fn proto_encode_event_wrapper(event: Event) -> BytesMut {
    let wrapper = proto::EventWrapper::from(event);
    let mut buffer = BytesMut::with_capacity(65536);
    wrapper.encode(&mut buffer).expect("prost encode should always succeed");
    buffer
}

/// Exhaustive test: verify MAX_NESTING_DEPTH is exactly right across all proto paths.
///
/// "Exactly right" means:
///   1. ALL paths roundtrip at MAX_NESTING_DEPTH (limit isn't too high)
///   2. At least one path fails decode at MAX_NESTING_DEPTH + 1 (limit isn't too low)
///
/// This is the single source of truth for the nesting limit. If a proto schema change
/// adds wrapper messages, prost changes its recursion limit, or a new Value-carrying
/// field is added, this test will fail and identify the broken path.
#[test]
fn max_nesting_depth_is_correct_for_all_proto_paths() {
    let max = super::super::ser::MAX_NESTING_DEPTH;
    let paths = all_proto_paths();

    // 1. Every path must roundtrip at MAX_NESTING_DEPTH.
    //    If this fails, the limit is too high — lower it or fix the proto schema.
    for path in &paths {
        // EventArray
        let buffer = proto_encode_event_array((path.make_event_array)(max));
        assert!(
            proto::EventArray::decode(buffer.freeze()).is_ok(),
            "EventArray decode FAILED at depth {max} for path: {name}\n\
             MAX_NESTING_DEPTH is too high for this path. \
             Lower the limit or check for new proto wrapper messages.",
            name = path.name,
        );

        // EventWrapper
        if let Some(make_wrapper) = path.make_event_wrapper {
            let buffer = proto_encode_event_wrapper(make_wrapper(max));
            assert!(
                proto::EventWrapper::decode(buffer.freeze()).is_ok(),
                "EventWrapper decode FAILED at depth {max} for path: {name}\n\
                 MAX_NESTING_DEPTH is too high for this path via EventWrapper.",
                name = path.name,
            );
        }
    }

    // 2. At least one path must fail at MAX_NESTING_DEPTH + 1.
    //    If no path fails, the limit is too low — raise it.
    let any_event_array_fails = paths.iter().any(|path| {
        let buffer = proto_encode_event_array((path.make_event_array)(max + 1));
        proto::EventArray::decode(buffer.freeze()).is_err()
    });
    let any_event_wrapper_fails = paths.iter().any(|path| {
        path.make_event_wrapper.map_or(false, |make_wrapper| {
            let buffer = proto_encode_event_wrapper(make_wrapper(max + 1));
            proto::EventWrapper::decode(buffer.freeze()).is_err()
        })
    });
    assert!(
        any_event_array_fails,
        "All EventArray paths succeeded at depth {}. MAX_NESTING_DEPTH ({}) could be raised.",
        max + 1, max,
    );
    assert!(
        any_event_wrapper_fails,
        "All EventWrapper paths succeeded at depth {}. MAX_NESTING_DEPTH ({}) could be raised.",
        max + 1, max,
    );
}

/// Verify the nesting gate rejects events at MAX_NESTING_DEPTH + 1 for all paths.
#[test]
fn nesting_gate_rejects_all_paths_above_max_depth() {
    let max = super::super::ser::MAX_NESTING_DEPTH;

    for path in all_proto_paths() {
        let array = (path.make_event_array)(max + 1);
        let mut buffer = BytesMut::with_capacity(8192);
        let result = array.encode(&mut buffer);
        assert!(
            matches!(result, Err(super::super::ser::EncodeError::NestingTooDeep { .. })),
            "nesting gate should reject depth {} for path: {}",
            max + 1,
            path.name,
        );
    }
}

/// Verify the nesting gate accepts events at exactly MAX_NESTING_DEPTH for all paths.
#[test]
fn nesting_gate_accepts_all_paths_at_max_depth() {
    let max = super::super::ser::MAX_NESTING_DEPTH;

    for path in all_proto_paths() {
        let array = (path.make_event_array)(max);
        let mut buffer = BytesMut::with_capacity(65536);
        assert!(
            array.encode(&mut buffer).is_ok(),
            "nesting gate should accept depth {max} for path: {}",
            path.name,
        );
    }
}

/// Verify flat events pass without issues.
#[test]
fn nesting_gate_accepts_flat_event() {
    let mut event = LogEvent::from("hello world");
    event.insert("foo", "bar");
    event.insert("count", 42);

    let events = EventArray::Logs(LogArray::from(vec![event]));
    let mut buffer = BytesMut::with_capacity(1024);
    assert!(events.encode(&mut buffer).is_ok());
}

/// Verify flat metrics pass without issues.
#[test]
fn nesting_gate_accepts_metric_events() {
    let metric = Metric::new(
        "test_counter",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    );
    let events = EventArray::Metrics(MetricArray::from(vec![metric]));
    let mut buffer = BytesMut::with_capacity(1024);
    assert!(events.encode(&mut buffer).is_ok());
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
