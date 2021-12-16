use bytes::BytesMut;
use pretty_assertions::assert_eq;
use quickcheck::{QuickCheck, TestResult};
use regex::Regex;

use super::*;
use crate::config::log_schema;

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
            let res = Event::decode(buffer);
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
            let res = Event::decode(buffer);
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
