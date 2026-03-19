use bytes::{Buf, BufMut, BytesMut};
use quickcheck::{QuickCheck, TestResult};
use regex::Regex;
use similar_asserts::assert_eq;
use vector_buffers::encoding::Encodable;
#[cfg(feature = "generate-fixtures")]
use {
    prost::Message,
    quickcheck::{Arbitrary as _, Gen},
    std::{fs::File, io::Write},
};

use super::*;
use crate::config::log_schema;

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

/// Generates fixture files used by the native encoding codec tests.
///
/// Run with:
///   cargo test -p vector-core --features generate-fixtures \
///       event::test::serialization::generate_fixtures -- --nocapture
///
/// The files are written directly to the fixture directory under
/// `lib/codecs/tests/data/native_encoding/{json,proto}/`.
#[cfg(feature = "generate-fixtures")]
#[test]
fn generate_fixtures() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = manifest_dir
        .join("../codecs/tests/data/native_encoding");
    let json_dir = fixture_dir.join("json");
    let proto_dir = fixture_dir.join("proto");
    std::fs::create_dir_all(&json_dir).unwrap();
    std::fs::create_dir_all(&proto_dir).unwrap();

    let mut generator = Gen::new(128);
    for n in 0..1024_usize {
        let mut json_out = File::create(json_dir.join(format!("{n:04}.json"))).unwrap();
        let mut proto_out = File::create(proto_dir.join(format!("{n:04}.pb"))).unwrap();
        let event = Event::arbitrary(&mut generator);
        serde_json::to_writer(&mut json_out, &event).unwrap();

        let array = EventArray::from(event);
        let proto = proto::EventArray::from(array);
        let mut buf = BytesMut::new();
        proto.encode(&mut buf).unwrap();
        proto_out.write_all(&buf).unwrap();
    }
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
