use bytes::BytesMut;
use pretty_assertions::assert_eq;
use quickcheck::{QuickCheck, TestResult};
use regex::Regex;
use shared::btreemap;

use super::*;
use crate::config::log_schema;

#[test]
fn event_encodable_metadata_stable_while_leveldb_disk_buffer_still_present() {
    // REVIEWERS: Be aware, if this is being removed or changed, the only acceptable context is
    // LevelDB-based disk buffers being removed, or some other extenuating circumstance that must be
    // explained.
    let allowed = allowed_event_encodable_metadata();
    let actual = <Event as Encodable>::get_metadata();
    assert_eq!(
        allowed, actual,
        "metadata for `Encodable` impl for `Event` must not change \
prior to the LevelDB-based disk buffer implementation being removed"
    );
}

#[test]
fn event_can_go_from_raw_prost_to_encodable_and_vice_versa() {
    // This is another test that ensures that we can encode via a raw Prost encode call and decode
    // via `Event`'s `Encodable` implementation, and vice versa, as an additional layer of assurance
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
    let event: Event = LogEvent::from_parts(event_fields, EventMetadata::default()).into();

    // First test: raw Prost encode -> `Encodable::decode`.
    let first_event = event.clone();

    let mut first_encode_buf = BytesMut::with_capacity(4096);
    proto::EventWrapper::from(first_event)
        .encode(&mut first_encode_buf)
        .expect("event should not fail to encode");

    let first_decode_buf = first_encode_buf.freeze();
    let first_decoded_event = Event::decode(Event::get_metadata(), first_decode_buf)
        .expect("event should not fail to decode");

    assert_eq!(event, first_decoded_event);

    // Second test: `Encodable::encode` -> raw Prost decode.
    let second_event = event.clone();

    let mut second_encode_buf = BytesMut::with_capacity(4096);
    second_event
        .encode(&mut second_encode_buf)
        .expect("event should not fail to encode");

    let second_decode_buf = second_encode_buf.freeze();
    let second_decoded_event: Event = proto::EventWrapper::decode(second_decode_buf)
        .map(Into::into)
        .expect("event should not fail to decode");

    assert_eq!(event, second_decoded_event);
}

// Ser/De the Event never loses bytes
#[test]
fn serde_no_size_loss() {
    fn inner(event: Event) -> TestResult {
        let expected = event.clone();

        let mut buffer = BytesMut::with_capacity(64);
        {
            let res = Event::encode(event, &mut buffer);
            assert!(res.is_ok());
        }
        {
            let res = Event::decode(Event::get_metadata(), buffer);
            let actual: Event = res.unwrap();

            assert_eq!(actual.size_of(), expected.size_of());
        }
        TestResult::passed()
    }

    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(Event) -> TestResult);
}

// Ser/De the Event type through EncodeBytes -> DecodeBytes
#[test]
#[allow(clippy::neg_cmp_op_on_partial_ord)] // satisfying clippy leads to less
                                            // clear expression
fn back_and_forth_through_bytes() {
    fn inner(event: Event) -> TestResult {
        let expected = event.clone();

        let mut buffer = BytesMut::with_capacity(64);
        {
            let res = Event::encode(event, &mut buffer);
            assert!(res.is_ok());
        }
        {
            let res = Event::decode(Event::get_metadata(), buffer);
            let actual: Event = res.unwrap();
            // While Event does implement PartialEq we prefer to use PartialOrd
            // instead. This is done because Event is populated with a number
            // f64 instances, meaning two Event instances might differ by less
            // than f64::EPSILON -- and are equal enough -- but are not
            // partially equal.
            assert!(!(expected > actual));
            assert!(!(expected < actual));
        }
        TestResult::passed()
    }

    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(Event) -> TestResult);
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

    let actual_all = serde_json::to_value(event.as_log().all_fields()).unwrap();
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

    let map = serde_json::to_value(event.as_log().all_fields()).unwrap();
    assert_eq!(map["float"], json!(5.5));
    assert_eq!(map["int"], json!(4));
    assert_eq!(map["bool"], json!(true));
    assert_eq!(map["string"], json!("thisisastring"));
}
