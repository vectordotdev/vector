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
// Prost enforces a decode recursion limit of 100 (no limit on encode). Each Value
// nesting level consumes 3 prost recursion entries (Value + ValueMap + map_entry),
// and each encoding path has a different number of proto wrapper messages before
// the Value tree starts:
//
//   - Event data path (Log.fields, Trace.fields): 3 wrappers → max depth 33
//   - Metadata path (metadata_full): 4 wrappers → max depth 32
//
// These limits are encoded as MAX_NESTING_DEPTH (33) and MAX_METADATA_NESTING_DEPTH (32).
// The `per_path_boundaries` test verifies both boundaries empirically via prost roundtrip.
//
// The saturated-event tests create events with ALL Value-carrying fields at their
// respective max depths simultaneously. The proto conversion code populates every field
// (including deprecated ones like Log.metadata), so a single roundtrip per event type
// covers every proto path automatically.

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

/// Verify both depth constants are exactly right: all event types roundtrip at the
/// max depths, and at least one fails prost decode when either limit is exceeded.
#[test]
fn max_nesting_depths_are_correct() {
    let max_val = super::super::ser::MAX_NESTING_DEPTH;
    let max_meta = super::super::ser::MAX_METADATA_NESTING_DEPTH;

    // --- Both limits at max must roundtrip for all event types ---

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

    // --- Exceeding either limit must fail for at least one event type ---

    // Exceed value depth
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
        "No path failed at value depth {}. MAX_NESTING_DEPTH could be raised.",
        max_val + 1
    );

    // Exceed metadata depth
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
        "No path failed at metadata depth {}. MAX_METADATA_NESTING_DEPTH could be raised.",
        max_meta + 1
    );
}

/// Verify the nesting gate accepts all event types at the max depths.
#[test]
fn nesting_gate_accepts_all_types_at_max_depth() {
    let max_val = super::super::ser::MAX_NESTING_DEPTH;
    let max_meta = super::super::ser::MAX_METADATA_NESTING_DEPTH;
    for (name, array) in saturated_event_arrays(max_val, max_meta) {
        let mut buf = BytesMut::with_capacity(65536);
        assert!(
            array.encode(&mut buf).is_ok(),
            "nesting gate rejected {name} at max depths",
        );
    }
}

/// Verify the nesting gate rejects when either limit is exceeded.
#[test]
fn nesting_gate_rejects_above_max_depth() {
    let max_val = super::super::ser::MAX_NESTING_DEPTH;
    let max_meta = super::super::ser::MAX_METADATA_NESTING_DEPTH;

    // Exceed value depth (Log and Trace have event data; Metric does not)
    for (name, array) in saturated_event_arrays(max_val + 1, max_meta) {
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
            "nesting gate should reject {name} at value depth {}",
            max_val + 1,
        );
    }

    // Exceed metadata depth
    for (name, array) in saturated_event_arrays(max_val, max_meta + 1) {
        let mut buf = BytesMut::with_capacity(65536);
        assert!(
            matches!(
                array.encode(&mut buf),
                Err(super::super::ser::EncodeError::NestingTooDeep { .. })
            ),
            "nesting gate should reject {name} at metadata depth {}",
            max_meta + 1,
        );
    }
}

/// Verify the per-path prost boundaries match the constants.
///
/// `Log.fields` (loosest): `MAX_NESTING_DEPTH` (33) succeeds, 34 fails.
/// `metadata_full` (tightest): `MAX_METADATA_NESTING_DEPTH` (32) succeeds, 33 fails.
#[test]
fn per_path_boundaries() {
    let max_val = super::super::ser::MAX_NESTING_DEPTH;
    let max_meta = super::super::ser::MAX_METADATA_NESTING_DEPTH;

    let roundtrip_value = |depth: usize| -> bool {
        let mut event = LogEvent::default();
        event.insert("data", create_nested_value(depth - 1));
        let array = EventArray::Logs(LogArray::from(vec![event]));
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        proto::EventArray::decode(buf.freeze()).is_ok()
    };

    let roundtrip_metadata = |depth: usize| -> bool {
        let mut event = LogEvent::from("flat");
        *event.metadata_mut().value_mut() = create_nested_value(depth);
        let array = EventArray::Logs(LogArray::from(vec![event]));
        let proto_array = proto::EventArray::from(array);
        let mut buf = BytesMut::with_capacity(65536);
        proto_array.encode(&mut buf).unwrap();
        proto::EventArray::decode(buf.freeze()).is_ok()
    };

    // Log.fields: MAX_NESTING_DEPTH succeeds, MAX_NESTING_DEPTH+1 fails
    assert!(
        roundtrip_value(max_val),
        "Log.fields should succeed at depth {max_val}"
    );
    assert!(
        !roundtrip_value(max_val + 1),
        "Log.fields should fail at depth {}",
        max_val + 1
    );

    // metadata_full: MAX_METADATA_NESTING_DEPTH succeeds, MAX_METADATA_NESTING_DEPTH+1 fails
    assert!(
        roundtrip_metadata(max_meta),
        "metadata_full should succeed at depth {max_meta}"
    );
    assert!(
        !roundtrip_metadata(max_meta + 1),
        "metadata_full should fail at depth {}",
        max_meta + 1
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
