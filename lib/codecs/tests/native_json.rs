#![allow(clippy::unwrap_used)]

use bytes::BytesMut;
use codecs::{
    NativeJsonDeserializerConfig, NativeJsonSerializerConfig, decoding::format::Deserializer,
    encoding::format::Serializer,
};
use vector_core::{
    buckets,
    config::LogNamespace,
    event::{Event, Metric, MetricKind, MetricValue},
};

fn assert_roundtrip(
    input_event: Event,
    serializer: &mut dyn Serializer<Error = vector_common::Error>,
    deserializer: &dyn Deserializer,
    expected_json_value: serde_json::Value,
) {
    let mut bytes_mut = BytesMut::new();
    serializer
        .encode(input_event.clone(), &mut bytes_mut)
        .unwrap();
    let bytes = bytes_mut.freeze();
    let events = deserializer.parse(bytes, LogNamespace::Vector).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], input_event);

    let json_value = serde_json::to_value(input_event.as_metric()).unwrap();
    assert_eq!(json_value, expected_json_value);
}

#[test]
fn histogram_metric_roundtrip() {
    let histogram_event = Event::from(Metric::new(
        "histogram",
        MetricKind::Absolute,
        MetricValue::AggregatedHistogram {
            count: 1,
            sum: Some(1.0),
            buckets: buckets!(
                f64::NEG_INFINITY => 10 ,
                f64::MIN => 10, 1.5 => 10,
                f64::MAX => 10,
                f64::INFINITY => 10),
        },
    ));

    let expected_json_value = serde_json::from_str(
        r#"
        {
            "aggregated_histogram":  {
                "buckets": [
                     {
                        "count": 10,
                        "upper_limit": "-inf"
                    },
                    {
                        "count": 10,
                        "upper_limit": -1.7976931348623157e308
                    },
                    {
                        "count": 10,
                        "upper_limit": 1.5
                    },
                    {
                        "count": 10,
                        "upper_limit": 1.7976931348623157e308
                    },
                     {
                        "count": 10,
                        "upper_limit": "inf"
                    }
                ],
                "count": 1,
                "sum": 1.0
            },
            "kind": "absolute",
            "name": "histogram"
        }"#,
    )
    .unwrap();

    assert_roundtrip(
        histogram_event,
        &mut NativeJsonSerializerConfig.build(),
        &NativeJsonDeserializerConfig::default().build(),
        expected_json_value,
    )
}

/// A histogram that reports no sum omits the key rather than emitting `"sum": null`, and an absent
/// key decodes back to an absent sum. Those two have to agree, or the value would not survive a
/// round-trip -- which is also why the field's serde default must be "absent" and not zero.
#[test]
fn histogram_without_sum_metric_roundtrip() {
    let histogram_event = Event::from(Metric::new(
        "histogram",
        MetricKind::Absolute,
        MetricValue::AggregatedHistogram {
            count: 3,
            sum: None,
            buckets: buckets!(1.0 => 1, 2.0 => 2),
        },
    ));

    let expected_json_value = serde_json::from_str(
        r#"
        {
            "aggregated_histogram":  {
                "buckets": [
                    {
                        "count": 1,
                        "upper_limit": 1.0
                    },
                    {
                        "count": 2,
                        "upper_limit": 2.0
                    }
                ],
                "count": 3
            },
            "kind": "absolute",
            "name": "histogram"
        }"#,
    )
    .unwrap();

    assert_roundtrip(
        histogram_event,
        &mut NativeJsonSerializerConfig.build(),
        &NativeJsonDeserializerConfig::default().build(),
        expected_json_value,
    )
}
