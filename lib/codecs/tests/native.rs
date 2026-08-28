#![allow(clippy::unwrap_used)]

use std::{
    fs::{self, File},
    io::{Read, Write},
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::Arc,
};

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use codecs::{
    NativeDeserializerConfig, NativeJsonDeserializerConfig, NativeJsonSerializerConfig,
    NativeSerializerConfig, decoding::format::Deserializer, encoding::format::Serializer,
};
use proptest::{
    collection::{btree_map, btree_set},
    prelude::*,
    test_runner::Config as ProptestConfig,
};
use similar_asserts::assert_eq;
use tokio_util::codec::Encoder;
use uuid::Uuid;
use vector_core::{
    config::{ComponentKey, LogNamespace, OutputId},
    event::{
        DatadogMetricOriginMetadata, Event, EventMetadata, LogEvent, Metric, MetricKind,
        MetricTags, MetricValue, ObjectMap, TraceEvent, Value, metric::TagValue,
    },
};
use vrl::event_path;

const PROPERTY_TESTS: u32 = 1_000;

fn bounded_string() -> BoxedStrategy<String> {
    proptest::collection::vec(any::<char>(), 0..16)
        .prop_map(|characters| characters.into_iter().collect())
        .boxed()
}

fn nonempty_bounded_string() -> BoxedStrategy<String> {
    proptest::collection::vec(any::<char>(), 1..16)
        .prop_map(|characters| characters.into_iter().collect())
        .boxed()
}

fn json_safe_value() -> BoxedStrategy<Value> {
    let leaf = prop_oneof![
        bounded_string().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        (-1_000_000.0_f64..=1_000_000.0).prop_map(|value| {
            let rounded = (value * 10_000.0).round() / 10_000.0;
            Value::from(if rounded == -0.0 { 0.0 } else { rounded })
        }),
        any::<bool>().prop_map(Value::from),
        Just(Value::Null),
    ];

    leaf.prop_recursive(3, 32, 4, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            btree_map(bounded_string(), inner, 0..4).prop_map(|entries| {
                Value::Object(
                    entries
                        .into_iter()
                        .map(|(key, value)| (key.into(), value))
                        .collect(),
                )
            }),
        ]
    })
    .boxed()
}

fn object_map() -> BoxedStrategy<ObjectMap> {
    btree_map(bounded_string(), json_safe_value(), 0..4)
        .prop_map(|entries| {
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect()
        })
        .boxed()
}

fn metric_value() -> BoxedStrategy<MetricValue> {
    prop_oneof![
        7 => any::<MetricValue>(),
        1 => btree_set(bounded_string(), 0..4)
            .prop_map(|values| MetricValue::Set { values }),
    ]
    .boxed()
}

fn metric_tags() -> BoxedStrategy<Option<MetricTags>> {
    let tag_value = prop_oneof![
        Just(TagValue::Bare),
        bounded_string().prop_map(TagValue::Value),
    ];

    proptest::option::of(btree_map(
        bounded_string(),
        proptest::collection::vec(tag_value, 0..4),
        1..4,
    ))
    .prop_map(|entries| {
        entries.map(|entries| {
            let mut tags = MetricTags::default();
            for (name, values) in entries {
                tags.set_multi_value(name, values);
            }
            tags
        })
    })
    .boxed()
}

fn event_metadata() -> BoxedStrategy<EventMetadata> {
    (
        json_safe_value(),
        proptest::option::of(bounded_string()),
        proptest::option::of(bounded_string()),
        proptest::option::of((bounded_string(), proptest::option::of(bounded_string()))),
        btree_map(bounded_string(), bounded_string(), 0..4),
        proptest::option::of((
            proptest::option::of(any::<u32>()),
            proptest::option::of(any::<u32>()),
            proptest::option::of(any::<u32>()),
        )),
        proptest::option::of(any::<[u8; 16]>().prop_map(Uuid::from_bytes)),
    )
        .prop_map(
            |(value, source_id, source_type, upstream_id, secrets, origin, source_event_id)| {
                let mut metadata =
                    EventMetadata::default_with_value(value).with_source_event_id(source_event_id);
                if let Some(source_id) = source_id {
                    metadata.set_source_id(Arc::new(ComponentKey::from(source_id)));
                }
                if let Some(source_type) = source_type {
                    metadata.set_source_type(source_type);
                }
                if let Some((component, port)) = upstream_id {
                    metadata.set_upstream_id(Arc::new(OutputId::from((component, port))));
                }
                for (key, value) in secrets {
                    metadata.secrets_mut().insert(key, value);
                }
                if let Some((product, category, service)) = origin {
                    metadata = metadata.with_origin_metadata(DatadogMetricOriginMetadata::new(
                        product, category, service,
                    ));
                }
                metadata
            },
        )
        .boxed()
}

fn timestamp() -> BoxedStrategy<Option<DateTime<Utc>>> {
    proptest::option::of(
        (-32_000_i64..=32_000, 0_u32..1_000_000_000).prop_map(|(seconds, nanoseconds)| {
            DateTime::from_timestamp(seconds, nanoseconds).unwrap()
        }),
    )
    .boxed()
}

fn interval() -> BoxedStrategy<Option<NonZeroU32>> {
    proptest::option::of((1_u32..=u32::MAX).prop_map(|value| NonZeroU32::new(value).unwrap()))
        .boxed()
}

fn event_strategy() -> BoxedStrategy<Event> {
    let metadata = event_metadata();
    let log = (object_map(), metadata.clone())
        .prop_map(|(fields, metadata)| Event::Log(LogEvent::from_map(fields, metadata)));
    let trace = (object_map(), metadata.clone())
        .prop_map(|(fields, metadata)| Event::Trace(TraceEvent::from_parts(fields, metadata)));
    let metric = (
        bounded_string(),
        prop_oneof![Just(MetricKind::Absolute), Just(MetricKind::Incremental)],
        metric_value(),
        metric_tags(),
        proptest::option::of(nonempty_bounded_string()),
        timestamp(),
        interval(),
        metadata,
    )
        .prop_map(
            |(name, kind, value, tags, namespace, timestamp, interval, metadata)| {
                Event::Metric(
                    Metric::new_with_metadata(name, kind, value, metadata)
                        .with_tags(tags)
                        .with_namespace(namespace)
                        .with_timestamp(timestamp)
                        .with_interval_ms(interval),
                )
            },
        );

    prop_oneof![log, metric, trace].boxed()
}

fn without_metadata(mut event: Event) -> Event {
    *event.metadata_mut() = EventMetadata::default();
    event
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROPERTY_TESTS))]

    #[test]
    fn native_proto_is_canonical_for_arbitrary_events(event in event_strategy()) {
        let expected = event.clone();
        let serializer = &mut NativeSerializerConfig.build();
        let mut encoded = BytesMut::new();
        serializer.encode(event, &mut encoded).unwrap();

        let mut decoded = NativeDeserializerConfig
            .build()
            .parse(encoded.clone().freeze(), LogNamespace::Legacy)
            .unwrap();
        prop_assert_eq!(decoded.len(), 1);
        let decoded = decoded.pop().unwrap();
        prop_assert_eq!(&decoded, &expected);

        let mut reencoded = BytesMut::new();
        serializer.encode(decoded, &mut reencoded).unwrap();

        prop_assert_eq!(encoded, reencoded);
    }

    #[test]
    fn native_json_is_canonical_for_arbitrary_events(event in event_strategy()) {
        let expected = without_metadata(event.clone());
        let serializer = &mut NativeJsonSerializerConfig.build();
        let mut encoded = BytesMut::new();
        serializer.encode(event, &mut encoded).unwrap();

        let mut decoded = NativeJsonDeserializerConfig::default()
            .build()
            .parse(encoded.clone().freeze(), LogNamespace::Legacy)
            .unwrap();
        prop_assert_eq!(decoded.len(), 1);
        let decoded = decoded.pop().unwrap();
        prop_assert_eq!(&decoded, &expected);

        let mut reencoded = BytesMut::new();
        serializer.encode(decoded, &mut reencoded).unwrap();

        prop_assert_eq!(encoded, reencoded);
    }
}

#[test]
fn native_json_decodes_legacy_u32_metric_counts() {
    let input = Bytes::from_static(
        br#"{"metric":{"name":"requests","kind":"absolute","aggregated_histogram":{"buckets":[{"upper_limit":1.0,"count":4294967295}],"count":4294967295,"sum":2.0}}}"#,
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
                count: u64::from(u32::MAX),
            }],
            count: u64::from(u32::MAX),
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

#[test]
fn pre_v24_fixtures_match() {
    fixtures_match("pre-v24");
}

#[test]
fn pre_v34_fixtures_match() {
    fixtures_match("pre-v34");
}

#[test]
fn pre_v41_fixtures_match() {
    fixtures_match("pre-v41");
}

#[test]
fn current_fixtures_match() {
    fixtures_match("");
}

#[test]
fn roundtrip_current_native_json_fixtures() {
    roundtrip_fixtures(
        "json",
        "",
        &NativeJsonDeserializerConfig::default().build(),
        &mut NativeJsonSerializerConfig.build(),
        false,
    );
}

#[test]
fn roundtrip_current_native_proto_fixtures() {
    roundtrip_fixtures(
        "proto",
        "",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
        false,
    );
}

/// The event proto file was changed in v0.24. This test ensures we can still load the old version
/// binary and that when serialized and deserialized in the new format we still get the same event.
#[test]
fn reserialize_pre_v24_native_json_fixtures() {
    roundtrip_fixtures(
        "json",
        "pre-v24",
        &NativeJsonDeserializerConfig::default().build(),
        &mut NativeJsonSerializerConfig.build(),
        true,
    );
}

#[test]
fn reserialize_pre_v24_native_proto_fixtures() {
    roundtrip_fixtures(
        "proto",
        "pre-v24",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
        true,
    );
}

/// The event proto format was changed in v26 to include support for enhanced metric tags. This test
/// ensures we can still load the old version binary and that when serialized and deserialized in
/// the new format we still get the same event.
#[test]
fn reserialize_pre_v26_native_proto_fixtures() {
    roundtrip_fixtures(
        "proto",
        "pre-v26",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
        true,
    );
}

/// The event proto file was changed in v0.34. This test ensures we can still load the old version
/// binary and that when serialized and deserialized in the new format we still get the same event.
#[test]
fn reserialize_pre_v34_native_json_fixtures() {
    roundtrip_fixtures(
        "json",
        "pre-v34",
        &NativeJsonDeserializerConfig::default().build(),
        &mut NativeJsonSerializerConfig.build(),
        true,
    );
}

#[test]
fn reserialize_pre_v34_native_proto_fixtures() {
    roundtrip_fixtures(
        "proto",
        "pre-v34",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
        true,
    );
}

/// The event proto file was changed in v0.41. This test ensures we can still load the old version
/// binary and that when serialized and deserialized in the new format we still get the same event.
#[test]
fn reserialize_pre_v41_native_json_fixtures() {
    roundtrip_fixtures(
        "json",
        "pre-v41",
        &NativeJsonDeserializerConfig::default().build(),
        &mut NativeJsonSerializerConfig.build(),
        true,
    );
}

#[test]
fn reserialize_pre_v41_native_proto_fixtures() {
    roundtrip_fixtures(
        "proto",
        "pre-v41",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
        true,
    );
}

// TODO: the json &  protobuf consistency has been broken for a while due to the lack of implementing
// serde deser and ser of EventMetadata. Thus the `native_json` codec is not passing through the
// `EventMetadata.value` field, whereas the `native` codec does.
//
// both of these tests are affected as a result
//
// https://github.com/vectordotdev/vector/issues/18570
#[ignore]
#[test]
fn pre_v34_native_decoding_matches() {
    decoding_matches("pre-v34");
}

#[ignore]
#[test]
fn pre_v41_native_decoding_matches() {
    decoding_matches("pre-v41");
}

#[ignore]
#[test]
fn current_native_decoding_matches() {
    decoding_matches("");
}

#[test]
fn pre_v24_native_decoding_matches() {
    decoding_matches("pre-v24");
}

/// This "test" can be used to build new protobuf fixture files when the protocol changes. Remove
/// the `#[ignore]` only when this is needed for such changes. You will need to manually create a
/// `tests/data/native_encoding/json/rebuilt` subdirectory for the files to be written to.
#[test]
#[ignore]
fn rebuild_json_fixtures() {
    rebuild_fixtures(
        "json",
        &NativeJsonDeserializerConfig::default().build(),
        &mut NativeJsonSerializerConfig.build(),
    );
}

/// This "test" can be used to build new protobuf fixture files when the protocol changes. Remove
/// the `#[ignore]` only when this is needed for such changes. You will need to manually create a
/// `tests/data/native_encoding/proto/rebuilt` subdirectory for the files to be written to.
#[test]
#[ignore]
fn rebuild_proto_fixtures() {
    rebuild_fixtures(
        "proto",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
    );
}

/// This test ensures that the different sets of protocol fixture names match.
fn fixtures_match(suffix: &str) {
    let json_entries = list_fixtures("json", suffix);
    let proto_entries = list_fixtures("proto", suffix);
    for (json_path, proto_path) in json_entries.into_iter().zip(proto_entries) {
        // Make sure we're looking at the matching files for each format
        assert_eq!(
            json_path.file_stem().unwrap(),
            proto_path.file_stem().unwrap(),
        );
    }
}

/// This test ensures we can load the serialized binaries binary and that they match across
/// protocols.
fn decoding_matches(suffix: &str) {
    let json_deserializer = NativeJsonDeserializerConfig::default().build();
    let proto_deserializer = NativeDeserializerConfig.build();

    let json_entries = list_fixtures("json", suffix);
    let proto_entries = list_fixtures("proto", suffix);

    for (json_path, proto_path) in json_entries.into_iter().zip(proto_entries) {
        let (_, json_event) = load_deserialize(&json_path, &json_deserializer);

        let (_, proto_event) = load_deserialize(&proto_path, &proto_deserializer);

        // Ensure that the json version and proto versions were parsed into equivalent
        // native representations
        assert_eq!(
            json_event,
            proto_event,
            "Parsed events don't match: {} {}",
            json_path.display(),
            proto_path.display()
        );
    }
}

fn list_fixtures(proto: &str, suffix: &str) -> Vec<PathBuf> {
    let path = fixtures_path(proto, suffix);
    let mut entries = fs::read_dir(path)
        .unwrap()
        .map(Result::unwrap)
        .filter(|e| e.file_type().unwrap().is_file())
        .map(|e| e.path())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn fixtures_path(proto: &str, suffix: &str) -> PathBuf {
    ["tests/data/native_encoding", proto, suffix]
        .into_iter()
        .collect()
}

fn roundtrip_fixtures(
    proto: &str,
    suffix: &str,
    deserializer: &dyn Deserializer,
    serializer: &mut dyn Serializer,
    reserialize: bool,
) {
    for path in list_fixtures(proto, suffix) {
        let (buf, event) = load_deserialize(&path, deserializer);

        if reserialize {
            // Serialize the parsed event
            let mut buf = BytesMut::new();
            serializer.encode(event.clone(), &mut buf).unwrap();
            // Deserialize the event from these bytes
            let new_events = deserializer
                .parse(buf.into(), LogNamespace::Legacy)
                .unwrap();

            // Ensure we have the same event.
            assert_eq!(new_events.len(), 1);
            assert_eq!(new_events[0], event);
        } else {
            // Ensure that the parsed event is serialized to the same bytes
            let mut new_buf = BytesMut::new();
            serializer.encode(event.clone(), &mut new_buf).unwrap();
            assert_eq!(buf, new_buf);
        }
    }
}

fn load_deserialize(path: &Path, deserializer: &dyn Deserializer) -> (Bytes, Event) {
    let mut file = File::open(path).unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    let buf = Bytes::from(buf);

    // Ensure that we can parse the json fixture successfully
    let mut events = deserializer
        .parse(buf.clone(), LogNamespace::Legacy)
        .unwrap();
    assert_eq!(events.len(), 1);
    (buf, events.pop().unwrap())
}

fn rebuild_fixtures(proto: &str, deserializer: &dyn Deserializer, serializer: &mut dyn Serializer) {
    for path in list_fixtures(proto, "") {
        let (_, event) = load_deserialize(&path, deserializer);

        let mut buf = BytesMut::new();
        serializer
            .encode(event, &mut buf)
            .expect("Serializing failed");

        let new_path: PathBuf = [
            fixtures_path(proto, "rebuilt"),
            path.file_name().unwrap().into(),
        ]
        .into_iter()
        .collect();
        let mut out = File::create(&new_path).unwrap_or_else(|error| {
            panic!("Could not create rebuilt file {new_path:?}: {error:?}")
        });
        out.write_all(&buf).expect("Could not write rebuilt data");
        out.flush().expect("Could not write rebuilt data");
    }
}
