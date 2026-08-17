use super::*;
use crate::config::log_schema;
use bytes::{Buf, BufMut, BytesMut};
use quickcheck::{QuickCheck, TestResult};
use regex::Regex;
use similar_asserts::assert_eq;
use vector_buffers::encoding::Encodable;
use vrl::event_path;

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

mod histogram_sum {
    use crate::event::metric::{Bucket, MetricValue};
    use crate::event::proto;
    use similar_asserts::assert_eq;

    fn histogram(sum: Option<f64>) -> MetricValue {
        MetricValue::AggregatedHistogram {
            buckets: vec![Bucket {
                upper_limit: 1.0,
                count: 3,
            }],
            count: 3,
            sum,
        }
    }

    fn encode(sum: Option<f64>) -> proto::metric::Value {
        proto::MetricValue::from(histogram(sum))
    }

    /// A histogram carrying a sum keeps encoding as `AggregatedHistogram3`, exactly as before this
    /// field became optional. That is what leaves the checked-in native-encoding fixtures
    /// byte-identical and lets an older peer keep decoding these.
    #[test]
    fn reported_sum_encodes_as_v3() {
        assert!(
            matches!(
                encode(Some(12.5)),
                proto::metric::Value::AggregatedHistogram3(_)
            ),
            "a histogram with a sum must still encode as AggregatedHistogram3"
        );
    }

    /// Only the case that `AggregatedHistogram3` cannot represent reaches for the new message.
    #[test]
    fn unreported_sum_encodes_as_v4() {
        assert!(
            matches!(encode(None), proto::metric::Value::AggregatedHistogram4(_)),
            "a histogram without a sum must encode as AggregatedHistogram4"
        );
    }

    #[test]
    fn both_survive_a_round_trip() {
        for sum in [Some(12.5), Some(0.0), None] {
            let decoded = MetricValue::from(encode(sum));
            assert_eq!(decoded, histogram(sum), "round-trip lost the sum {sum:?}");
        }
    }

    /// Versions 1 through 3 have no way to say "no sum", so whatever they carry was reported --
    /// including a zero, which proto3 implicit presence does not even write to the wire. Reading
    /// those as `None` would silently reinterpret every previously encoded zero.
    #[test]
    fn legacy_versions_decode_as_a_reported_sum() {
        let bucket3 = proto::HistogramBucket3 {
            upper_limit: 1.0,
            count: 3,
        };

        let v3 = proto::metric::Value::AggregatedHistogram3(proto::AggregatedHistogram3 {
            buckets: vec![bucket3.clone()],
            count: 3,
            sum: 0.0,
        });
        assert_eq!(MetricValue::from(v3), histogram(Some(0.0)));

        let v2 = proto::metric::Value::AggregatedHistogram2(proto::AggregatedHistogram2 {
            buckets: vec![proto::HistogramBucket {
                upper_limit: 1.0,
                count: 3,
            }],
            count: 3,
            sum: 0.0,
        });
        assert_eq!(MetricValue::from(v2), histogram(Some(0.0)));

        let v1 = proto::metric::Value::AggregatedHistogram1(proto::AggregatedHistogram1 {
            buckets: vec![1.0],
            counts: vec![3],
            count: 3,
            sum: 0.0,
        });
        assert_eq!(MetricValue::from(v1), histogram(Some(0.0)));
    }

    /// `PartialEq` has to keep these apart, or none of the above proves anything.
    #[test]
    fn an_unreported_sum_differs_from_zero() {
        assert_ne!(histogram(None), histogram(Some(0.0)));
        assert_eq!(histogram(None), histogram(None));
    }
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
