use bytes::{Buf, BufMut, BytesMut};
use pretty_assertions::assert_eq;
use prost::Message;
use quickcheck::{QuickCheck, TestResult};
use regex::Regex;
use vector_buffers::encoding::Encodable;
use vector_common::btreemap;

use super::*;
use crate::{config::log_schema, event::ser::EventEncodableMetadataFlags};

fn encode_value<T: Encodable, B: BufMut>(value: T, buffer: &mut B) {
    value.encode(buffer).expect("encoding should not fail");
}

fn decode_value<T: Encodable, B: Buf + Clone>(buffer: B) -> T {
    T::decode(T::get_metadata(), buffer).expect("decoding should not fail")
}

#[test]
fn encodable_must_decode_single_eventarray_while_leveldb_disk_buffer_still_present() {
    // REVIEWERS: Be aware, if this is being removed or changed, the only acceptable context is
    // LevelDB-based disk buffers being removed, or some other extenuating circumstance that must be
    // explained.
    let metadata = <EventArray as Encodable>::get_metadata();
    assert!(
        <EventArray as Encodable>::can_decode(metadata),
        "metadata for `Encodable` impl for `EventArray` must support decoding individual \
events prior to the LevelDB-based disk buffer implementation being removed"
    );
}

#[test]
fn eventarray_can_go_from_raw_prost_to_encodable_and_vice_versa() {
    // This is another test that ensures that we can encode via a raw Prost encode call and decode
    // via `EventArray`'s `Encodable` implementation, and vice versa, as an additional layer of assurance
    // that we haven't changed the `Encodable` implementation prior to removing the LevelDB-based
    // disk buffers.
    //
    // REVIEWERS: Be aware, if this is being removed or changed, the only acceptable context is
    // LevelDB-based disk buffers being removed, or some other extenuating circumstance that must be
    // explained.

    let event_fields = btreemap! {
        "key1" => "value1",
        "key2" => "value2",
        "key3" => "value3",
    };
    let event: Event = LogEvent::from_map(event_fields, EventMetadata::default()).into();
    let events = EventArray::from(event);

    // First test: raw Prost encode -> `Encodable::decode`.
    let mut first_encode_buf = BytesMut::with_capacity(4096);
    proto::EventArray::from(events.clone())
        .encode(&mut first_encode_buf)
        .expect("events should not fail to encode");

    let first_decode_buf = first_encode_buf.freeze();
    let first_decoded = EventArray::decode(EventArray::get_metadata(), first_decode_buf)
        .expect("events should not fail to decode");

    assert_eq!(events, first_decoded);

    // Second test: `Encodable::encode` -> raw Prost decode.
    let mut second_encode_buf = BytesMut::with_capacity(4096);
    events
        .clone()
        .encode(&mut second_encode_buf)
        .expect("events should not fail to encode");

    let second_decode_buf = second_encode_buf.freeze();
    let second_decoded: EventArray = proto::EventArray::decode(second_decode_buf)
        .map(Into::into)
        .expect("events should not fail to decode");

    assert_eq!(events, second_decoded);
}

#[test]
fn event_can_go_from_raw_prost_to_eventarray_encodable() {
    // This is another test that ensures that we can encode via a raw Prost encode call and decode
    // via `EventArray`'s `Encodable` implementation, specifically for a single `Event`.  This is
    // the invariant we must provide to ensure that older disk buffer v1 files are still readable
    // when `EventArray` is introduced.
    //
    // REVIEWERS: Be aware, if this is being removed or changed, the only acceptable context is
    // LevelDB-based disk buffers being removed, or some other extenuating circumstance that must be
    // explained.

    let event_fields = btreemap! {
        "key1" => "value1",
        "key2" => "value2",
        "key3" => "value3",
    };
    let event: Event = LogEvent::from_map(event_fields, EventMetadata::default()).into();

    let mut encode_buf = BytesMut::with_capacity(4096);
    proto::EventWrapper::from(event.clone())
        .encode(&mut encode_buf)
        .expect("event should not fail to encode");

    let decode_buf = encode_buf.freeze();
    let decoded_events = EventArray::decode(
        EventEncodableMetadataFlags::DiskBufferV1CompatibilityMode.into(),
        decode_buf,
    )
    .expect("event should not fail to decode");

    let mut events = decoded_events.into_events().collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(event, events.remove(0));
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

        // While Event does implement PartialEq we prefer to use PartialOrd
        // instead. This is done because Event is populated with a number
        // f64 instances, meaning two Event instances might differ by less
        // than f64::EPSILON -- and are equal enough -- but are not
        // partially equal.
        assert!(!(expected > actual));
        assert!(!(expected < actual));

        TestResult::passed()
    }

    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(EventArray) -> TestResult);
}

#[test]
fn serialization() {
    let mut event = Event::from("raw log line");
    event.as_mut_log().insert("foo", "bar");
    event.as_mut_log().insert("bar", "baz");

    let expected_all = serde_json::json!({
        "message": "raw log line",
        "foo": "bar",
        "bar": "baz",
        "timestamp": event.as_log().get(log_schema().timestamp_key()),
    });

    let actual_all = serde_json::to_value(event.as_log().all_fields().unwrap()).unwrap();
    assert_eq!(expected_all, actual_all);

    let rfc3339_re = Regex::new(r"\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z\z").unwrap();
    assert!(rfc3339_re.is_match(actual_all.pointer("/timestamp").unwrap().as_str().unwrap()));
}

#[test]
fn type_serialization() {
    use serde_json::json;

    let mut event = Event::from("hello world");
    event.as_mut_log().insert("int", 4);
    event.as_mut_log().insert("float", 5.5);
    event.as_mut_log().insert("bool", true);
    event.as_mut_log().insert("string", "thisisastring");

    let map = serde_json::to_value(event.as_log().all_fields().unwrap()).unwrap();
    assert_eq!(map["float"], json!(5.5));
    assert_eq!(map["int"], json!(4));
    assert_eq!(map["bool"], json!(true));
    assert_eq!(map["string"], json!("thisisastring"));
}
