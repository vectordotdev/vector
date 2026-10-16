//! `encoding.json.bytes_format: base64` combined with every encoding option a sink can be configured
//! with.
//!
//! With `base64`, the output must be byte-identical to the default output for the same event
//! after replacing every byte value with its base64 text: only the byte values change, while the
//! transformer, framing, batch wrapping, pretty output, and metric handling behave as with the
//! default.
#![allow(clippy::unwrap_used)]

use base64::{Engine, engine::general_purpose::STANDARD};
use bytes::{Bytes, BytesMut};
use chrono::{TimeZone, Utc};
use codecs::encoding::{Encoder, EncodingConfigWithFraming, Framer, SinkType};
use serde_json::json;
use tokio_util::codec::Encoder as _;
use vector_core::event::{
    Event, LogEvent, Metric, MetricKind, MetricValue, ObjectMap, TraceEvent, Value,
};
use vrl::btreemap;

fn binary(len: usize, seed: u8) -> Value {
    Value::Bytes(Bytes::from(
        (0..len)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect::<Vec<u8>>(),
    ))
}

fn nested(levels: usize) -> Value {
    (0..levels).fold(binary(9, 7), |value, level| {
        if level % 2 == 0 {
            Value::from(btreemap! { "level" => value, "text" => Value::from("text") })
        } else {
            Value::Array(vec![value, Value::from(level as i64)])
        }
    })
}

fn events() -> Vec<(&'static str, Event)> {
    let timestamp = Value::from(Utc.with_ymd_and_hms(2026, 9, 30, 12, 0, 0).unwrap());
    vec![
        (
            "log with every value type",
            Event::Log(LogEvent::from(btreemap! {
                "message" => binary(48, 0xd3),
                "source_type" => Value::from("websocket"),
                "timestamp" => timestamp.clone(),
                "nested" => Value::from(btreemap! {
                    "inner" => binary(3, 1),
                    "list" => Value::Array(vec![binary(2, 2), Value::from(1), Value::Null]),
                    "empty_object" => Value::Object(ObjectMap::new()),
                }),
                "empty_array" => Value::Array(Vec::new()),
                "empty_string" => Value::from(""),
                "float" => Value::from(1.5),
                "boolean" => Value::from(false),
                "é \"quoted\"\nkey" => Value::from("ünïcode ✓"),
                "regex" => Value::Regex(vrl::value::ValueRegex::new(
                    regex::Regex::new("a+b").unwrap().into(),
                )),
            })),
        ),
        (
            "bytes root (Vector log namespace)",
            Event::Log(LogEvent::from(binary(4096, 5))),
        ),
        (
            "array root",
            Event::Log(LogEvent::from(Value::Array(vec![
                binary(4, 3),
                Value::from(2),
            ]))),
        ),
        ("integer root", Event::Log(LogEvent::from(Value::from(42)))),
        (
            "deeply nested",
            Event::Log(LogEvent::from(btreemap! {
                "root" => nested(32),
                "timestamp" => timestamp.clone(),
            })),
        ),
        (
            "trace",
            Event::Trace(TraceEvent::from(LogEvent::from(btreemap! {
                "trace_id" => Value::from(123),
                "span_id" => binary(8, 9),
                "spans" => Value::Array(vec![Value::from(btreemap! {
                    "service" => Value::from("service"),
                    "meta" => Value::from(btreemap! { "blob" => binary(16, 4) }),
                })]),
                "timestamp" => timestamp,
            }))),
        ),
        (
            "metric",
            Event::Metric(
                Metric::new(
                    "requests",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_tags(Some(vector_core::metric_tags!(
                    "a" => "1",
                    "a" => "2",
                    "b" => "x",
                ))),
            ),
        ),
    ]
}

/// Replaces every byte value with its base64 text: the expected effect of `bytes: base64`.
fn encode_bytes_as_base64(value: &mut Value) {
    match value {
        Value::Bytes(bytes) => *bytes = STANDARD.encode(&*bytes).into(),
        Value::Object(map) => map.values_mut().for_each(encode_bytes_as_base64),
        Value::Array(array) => array.iter_mut().for_each(encode_bytes_as_base64),
        _ => {}
    }
}

fn framings() -> Vec<serde_json::Value> {
    vec![
        serde_json::Value::Null,
        json!({ "method": "bytes" }),
        json!({ "method": "newline_delimited" }),
        json!({ "method": "character_delimited", "character_delimited": { "delimiter": "," } }),
        json!({ "method": "character_delimited", "character_delimited": { "delimiter": "\t" } }),
        json!({ "method": "length_delimited", "length_delimited": {} }),
        json!({ "method": "varint_length_delimited", "varint_length_delimited": {} }),
    ]
}

fn transformers() -> Vec<serde_json::Value> {
    let mut transformers = vec![
        json!({}),
        json!({ "only_fields": ["message", "nested.inner", "spans"] }),
        json!({ "except_fields": ["source_type", "nested.list"] }),
    ];
    for format in [
        "unix",
        "rfc3339",
        "unix_ms",
        "unix_us",
        "unix_ns",
        "unix_float",
    ] {
        transformers.push(json!({ "timestamp_format": format }));
    }
    transformers
}

fn config(
    bytes_format: &str,
    pretty: bool,
    metric_tag_values: &str,
    transformer: &serde_json::Value,
    framing: &serde_json::Value,
) -> EncodingConfigWithFraming {
    let mut encoding = json!({
        "codec": "json",
        "json": { "pretty": pretty, "bytes_format": bytes_format },
        "metric_tag_values": metric_tag_values,
    });
    let encoding_options = encoding.as_object_mut().unwrap();
    encoding_options.extend(transformer.as_object().unwrap().clone());
    let mut config = json!({ "encoding": encoding });
    if !framing.is_null() {
        config["framing"] = framing.clone();
    }
    serde_json::from_value(config).unwrap()
}

fn sink_type(stream_based: bool) -> SinkType {
    if stream_based {
        SinkType::StreamBased
    } else {
        SinkType::MessageBased
    }
}

#[test]
fn base64_only_changes_byte_values_for_every_encoding_option() {
    let events = events();
    let mut checked = 0;
    for framing in framings() {
        for transformer in transformers() {
            for pretty in [false, true] {
                for metric_tag_values in ["single", "full"] {
                    let base64 =
                        config("base64", pretty, metric_tag_values, &transformer, &framing);
                    let default = config(
                        "lossy_utf8",
                        pretty,
                        metric_tag_values,
                        &transformer,
                        &framing,
                    );

                    for stream_based in [true, false] {
                        let context = format!(
                            "framing {framing}, transformer {transformer}, pretty {pretty}, \
                             metric_tag_values {metric_tag_values}, stream_based {stream_based}"
                        );
                        let (framer, base64_serializer) =
                            base64.build(sink_type(stream_based)).unwrap();
                        let mut base64_encoder =
                            Encoder::<Framer>::new(framer, base64_serializer.clone());
                        let (framer, default_serializer) =
                            default.build(sink_type(stream_based)).unwrap();
                        let mut default_encoder =
                            Encoder::<Framer>::new(framer, default_serializer.clone());

                        assert_eq!(
                            base64_encoder.batch_prefix(),
                            default_encoder.batch_prefix()
                        );
                        assert_eq!(
                            base64_encoder.batch_suffix(false),
                            default_encoder.batch_suffix(false)
                        );
                        assert_eq!(
                            base64_encoder.content_type(),
                            default_encoder.content_type()
                        );

                        for (name, event) in &events {
                            // What a sink does: transform the event, then encode it.
                            let mut actual_event = event.clone();
                            base64.transformer().transform(&mut actual_event);
                            let mut actual = BytesMut::new();
                            base64_encoder
                                .encode(actual_event.clone(), &mut actual)
                                .unwrap();

                            let mut expected_event = event.clone();
                            default.transformer().transform(&mut expected_event);
                            match &mut expected_event {
                                Event::Log(log) => encode_bytes_as_base64(log.value_mut()),
                                Event::Trace(trace) => encode_bytes_as_base64(trace.value_mut()),
                                Event::Metric(_) => {}
                            }
                            let mut expected = BytesMut::new();
                            default_encoder
                                .encode(expected_event.clone(), &mut expected)
                                .unwrap();

                            assert_eq!(actual, expected, "encode {name}: {context}");
                            assert_eq!(
                                base64_serializer.to_json_value(actual_event).unwrap(),
                                default_serializer.to_json_value(expected_event).unwrap(),
                                "to_json_value {name}: {context}"
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(checked, 7 * 9 * 2 * 2 * 2 * events.len());
}
