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

/// Create a `LogEvent` with the specified nesting depth of maps.
///
/// The resulting `LogEvent` has a root Object (depth 0) containing a "data" key
/// whose value is `wrapping_levels` levels of nested maps with a string leaf.
/// The leaf string is at effective depth `wrapping_levels + 1` from the root.
fn create_nested_log_event(wrapping_levels: usize) -> LogEvent {
    let mut value = Value::from("innermost");
    for _ in 0..wrapping_levels {
        let mut map = ObjectMap::new();
        map.insert("nested".into(), value);
        value = Value::Object(map);
    }
    let mut event = LogEvent::default();
    event.insert("data", value);
    event
}

/// The vector sink encodes events as `EventWrapper` (not `EventArray`), which has a
/// different proto structure. Verify that it can also decode at `MAX_NESTING_DEPTH`.
#[test]
fn event_wrapper_path_safe_at_max_depth() {
    let event = create_nested_log_event(super::super::ser::MAX_NESTING_DEPTH - 1);
    let wrapper = proto::EventWrapper::from(Event::Log(event));

    let mut buffer = BytesMut::with_capacity(65536);
    wrapper.encode(&mut buffer).unwrap();
    let result = proto::EventWrapper::decode(buffer.freeze());
    assert!(
        result.is_ok(),
        "EventWrapper path should succeed at MAX_NESTING_DEPTH"
    );
}

/// Demonstrates the root cause: prost encodes deeply nested events successfully,
/// but fails to decode them due to its internal recursion limit of 100.
#[test]
fn deeply_nested_event_encodes_but_fails_prost_decode() {
    // 33 wrapping levels exceeds the prost decode limit
    let event = create_nested_log_event(33);
    let array = EventArray::Logs(LogArray::from(vec![event]));

    // Bypass our nesting check: convert directly to proto and encode raw
    let proto_array = proto::EventArray::from(array);
    let mut buffer = BytesMut::with_capacity(16384);
    proto_array
        .encode(&mut buffer)
        .expect("prost encode should succeed even for deeply nested data");

    // Decode fails: prost hits its recursion limit
    let result = proto::EventArray::decode(buffer.freeze());
    assert!(
        result.is_err(),
        "prost decode should fail for events exceeding the recursion limit"
    );
}

/// Confirms that events at exactly the max allowed depth encode AND decode via raw prost.
#[test]
fn event_at_max_depth_roundtrips_via_prost() {
    // 31 wrapping levels + "data" key = leaf at depth 32 = MAX_NESTING_DEPTH
    let event = create_nested_log_event(31);
    let original = event.clone();
    let array = EventArray::Logs(LogArray::from(vec![event]));

    let proto_array = proto::EventArray::from(array);
    let mut buffer = BytesMut::with_capacity(8192);
    proto_array
        .encode(&mut buffer)
        .expect("prost encode should succeed at MAX_NESTING_DEPTH");

    let decoded_proto = proto::EventArray::decode(buffer.freeze())
        .expect("prost decode should succeed at MAX_NESTING_DEPTH");
    let decoded_array = EventArray::from(decoded_proto);

    let decoded_event = decoded_array.into_events().next().unwrap().into_log();
    assert_eq!(
        decoded_event.value().get("data"),
        original.value().get("data"),
    );
}

#[test]
fn nesting_gate_accepts_flat_event() {
    let mut event = LogEvent::from("hello world");
    event.insert("foo", "bar");
    event.insert("count", 42);

    let events = EventArray::Logs(LogArray::from(vec![event]));
    let mut buffer = BytesMut::with_capacity(1024);
    assert!(events.encode(&mut buffer).is_ok());
}

#[test]
fn nesting_gate_accepts_event_at_max_depth() {
    // 31 wrapping levels + "data" key = leaf at depth 32 = MAX_NESTING_DEPTH
    let event = create_nested_log_event(31);
    let original = event.clone();
    let events = EventArray::Logs(LogArray::from(vec![event]));
    let mut buffer = BytesMut::with_capacity(8192);

    events
        .encode(&mut buffer)
        .expect("encode should succeed at exactly MAX_NESTING_DEPTH");

    let decoded = EventArray::decode(EventArray::get_metadata(), buffer)
        .expect("decode should succeed at exactly MAX_NESTING_DEPTH");

    let decoded_event = decoded.into_events().next().unwrap().into_log();
    assert_eq!(
        decoded_event.value().get("data"),
        original.value().get("data"),
    );
}

#[test]
fn nesting_gate_rejects_event_exceeding_max_depth() {
    // 32 wrapping levels + "data" key = leaf at depth 33, exceeds MAX_NESTING_DEPTH (32)
    let event = create_nested_log_event(32);
    let events = EventArray::Logs(LogArray::from(vec![event]));
    let mut buffer = BytesMut::with_capacity(8192);

    let result = events.encode(&mut buffer);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(
            err,
            super::super::ser::EncodeError::NestingTooDeep {
                depth: 33,
                max_depth: 32
            }
        ),
        "expected NestingTooDeep error, got: {err:?}"
    );
}

#[test]
fn nesting_gate_rejects_trace_event_exceeding_max_depth() {
    let mut value = Value::from("innermost");
    for _ in 0..32 {
        let mut map = ObjectMap::new();
        map.insert("nested".into(), value);
        value = Value::Object(map);
    }
    let mut trace = TraceEvent::default();
    trace.insert("data", value);

    let events = EventArray::Traces(TraceArray::from(vec![trace]));
    let mut buffer = BytesMut::with_capacity(8192);

    let result = events.encode(&mut buffer);
    assert!(matches!(
        result,
        Err(super::super::ser::EncodeError::NestingTooDeep { .. })
    ));
}

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
