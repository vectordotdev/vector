#![allow(clippy::unwrap_used)]

use std::{num::NonZeroU32, sync::Arc};

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use codecs::{
    NativeDeserializerConfig, NativeJsonDeserializerConfig, NativeJsonSerializerConfig,
    NativeSerializerConfig, decoding::format::Deserializer,
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
        MetricTags, MetricValue, ObjectMap, TraceEvent, Value,
        metric::{Bucket, Quantile, TagValue},
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

fn json_safe_leaf() -> BoxedStrategy<Value> {
    prop_oneof![
        bounded_string().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        (-1_000_000.0_f64..=1_000_000.0).prop_map(|value| {
            let rounded = (value * 10_000.0).round() / 10_000.0;
            Value::from(if rounded == -0.0 { 0.0 } else { rounded })
        }),
        any::<bool>().prop_map(Value::from),
        Just(Value::Null),
    ]
    .boxed()
}

fn value_strategy(leaf: BoxedStrategy<Value>) -> BoxedStrategy<Value> {
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

fn json_safe_value() -> BoxedStrategy<Value> {
    value_strategy(json_safe_leaf())
}

fn datetime() -> BoxedStrategy<DateTime<Utc>> {
    (-32_000_i64..=32_000, 0_u32..1_000_000_000)
        .prop_map(|(seconds, nanoseconds)| DateTime::from_timestamp(seconds, nanoseconds).unwrap())
        .boxed()
}

fn proto_value() -> BoxedStrategy<Value> {
    value_strategy(
        prop_oneof![
            5 => json_safe_leaf(),
            1 => datetime().prop_map(Value::Timestamp),
        ]
        .boxed(),
    )
}

fn object_map(value: BoxedStrategy<Value>) -> BoxedStrategy<ObjectMap> {
    btree_map(bounded_string(), value, 0..4)
        .prop_map(|entries| {
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect()
        })
        .boxed()
}

fn metric_float() -> BoxedStrategy<f64> {
    (proptest::num::f64::POSITIVE | proptest::num::f64::NEGATIVE | proptest::num::f64::ZERO).boxed()
}

fn quantile_value() -> BoxedStrategy<f64> {
    (0_u32..=10_000)
        .prop_map(|value| f64::from(value) / 10_000.0)
        .boxed()
}

fn metric_value() -> BoxedStrategy<MetricValue> {
    prop_oneof![
        7 => any::<MetricValue>(),
        1 => btree_set(bounded_string(), 0..4)
            .prop_map(|values| MetricValue::Set { values }),
        1 => (
            proptest::collection::vec(
                (metric_float(), any::<u64>())
                    .prop_map(|(upper_limit, count)| Bucket { upper_limit, count }),
                0..8,
            ),
            any::<u64>(),
            metric_float(),
        ).prop_map(|(buckets, count, sum)| MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        }),
        1 => (
            proptest::collection::vec(
                (quantile_value(), metric_float())
                    .prop_map(|(quantile, value)| Quantile { quantile, value }),
                0..8,
            ),
            any::<u64>(),
            metric_float(),
        ).prop_map(|(quantiles, count, sum)| MetricValue::AggregatedSummary {
            quantiles,
            count,
            sum,
        }),
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

fn event_metadata(value: BoxedStrategy<Value>) -> BoxedStrategy<EventMetadata> {
    (
        value,
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
    proptest::option::of(datetime()).boxed()
}

fn interval() -> BoxedStrategy<Option<NonZeroU32>> {
    proptest::option::of((1_u32..=u32::MAX).prop_map(|value| NonZeroU32::new(value).unwrap()))
        .boxed()
}

fn event_strategy(value: BoxedStrategy<Value>) -> BoxedStrategy<Event> {
    let metadata = event_metadata(value.clone());
    let log = (object_map(value.clone()), metadata.clone())
        .prop_map(|(fields, metadata)| Event::Log(LogEvent::from_map(fields, metadata)));
    let trace = (object_map(value), metadata.clone())
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
    fn native_proto_is_canonical_for_arbitrary_events(event in event_strategy(proto_value())) {
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
        prop_assert_eq!(
            decoded.metadata().source_event_id(),
            expected.metadata().source_event_id()
        );
        prop_assert_eq!(&decoded, &expected);

        let mut reencoded = BytesMut::new();
        serializer.encode(decoded, &mut reencoded).unwrap();

        prop_assert_eq!(encoded, reencoded);
    }

    #[test]
    fn native_json_is_canonical_for_arbitrary_events(event in event_strategy(json_safe_value())) {
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
