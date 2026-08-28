#![allow(clippy::unwrap_used)]

use bytes::{Bytes, BytesMut};
use codecs::{
    NativeDeserializerConfig, NativeJsonDeserializerConfig, NativeJsonSerializerConfig,
    NativeSerializerConfig, decoding::format::Deserializer,
};
use quickcheck::{QuickCheck, TestResult};
use tokio_util::codec::Encoder;
use vector_core::{
    config::LogNamespace,
    event::{Event, MetricValue},
};
use vrl::event_path;

const PROPERTY_TESTS: u64 = 1_000;

#[test]
fn native_proto_is_canonical_for_arbitrary_events() {
    fn canonicalizes(event: Event) -> TestResult {
        let serializer = &mut NativeSerializerConfig.build();
        let mut encoded = BytesMut::new();
        serializer.encode(event, &mut encoded).unwrap();

        let mut decoded = NativeDeserializerConfig
            .build()
            .parse(encoded.clone().freeze(), LogNamespace::Legacy)
            .unwrap();
        if decoded.len() != 1 {
            return TestResult::failed();
        }

        let mut reencoded = BytesMut::new();
        serializer
            .encode(decoded.pop().unwrap(), &mut reencoded)
            .unwrap();

        TestResult::from_bool(encoded == reencoded)
    }

    QuickCheck::new()
        .tests(PROPERTY_TESTS)
        .quickcheck(canonicalizes as fn(Event) -> TestResult);
}

#[test]
fn native_json_is_canonical_for_arbitrary_events() {
    fn canonicalizes(event: Event) -> TestResult {
        let serializer = &mut NativeJsonSerializerConfig.build();
        let mut encoded = BytesMut::new();
        serializer.encode(event, &mut encoded).unwrap();

        let mut decoded = NativeJsonDeserializerConfig::default()
            .build()
            .parse(encoded.clone().freeze(), LogNamespace::Legacy)
            .unwrap();
        if decoded.len() != 1 {
            return TestResult::failed();
        }

        let mut reencoded = BytesMut::new();
        serializer
            .encode(decoded.pop().unwrap(), &mut reencoded)
            .unwrap();

        TestResult::from_bool(encoded == reencoded)
    }

    QuickCheck::new()
        .tests(PROPERTY_TESTS)
        .quickcheck(canonicalizes as fn(Event) -> TestResult);
}

#[test]
fn native_json_decodes_legacy_u32_metric_counts() {
    let input = Bytes::from_static(
        br#"{"metric":{"name":"requests","kind":"absolute","aggregated_histogram":{"buckets":[{"upper_limit":1.0,"count":2}],"count":2,"sum":2.0}}}"#,
    );

    let mut events = NativeJsonDeserializerConfig::default()
        .build()
        .parse(input, LogNamespace::Legacy)
        .unwrap();
    let metric = events.pop().unwrap().into_metric();

    assert_eq!(
        metric.value(),
        &MetricValue::AggregatedHistogram {
            buckets: vec![vector_core::event::metric::Bucket {
                upper_limit: 1.0,
                count: 2,
            }],
            count: 2,
            sum: 2.0,
        }
    );
}

#[test]
fn native_json_decodes_events_without_metadata() {
    let input = Bytes::from_static(br#"{"log":{"message":"legacy"}}"#);

    let mut events = NativeJsonDeserializerConfig::default()
        .build()
        .parse(input, LogNamespace::Legacy)
        .unwrap();
    let log = events.pop().unwrap().into_log();

    assert_eq!(
        log.get(event_path!("message")),
        Some(&vector_core::event::Value::from("legacy"))
    );
    assert_eq!(
        log.metadata().value(),
        &vector_core::event::Value::Object(Default::default())
    );
}
