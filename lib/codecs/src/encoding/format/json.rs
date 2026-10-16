use base64::{display::Base64Display, engine::general_purpose::STANDARD};
use bytes::{BufMut, BytesMut};
use serde::{Serialize, Serializer};
use tokio_util::codec::Encoder;
use vector_config_macros::configurable_component;
use vector_core::{
    config::DataType,
    event::{Event, Value},
    schema,
};

use crate::MetricTagValues;

/// Config used to build a `JsonSerializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct JsonSerializerConfig {
    /// Controls how metric tag values are encoded.
    ///
    /// When set to `single`, only the last non-bare value of tags are displayed with the
    /// metric. When set to `full`, all metric tags are exposed as separate assignments.
    /// When set to `auto`, tag values are encoded using their underlying shape.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub metric_tag_values: MetricTagValues,

    /// Options for the JsonSerializer.
    #[serde(default, rename = "json")]
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::doc_markdown,
        reason = "Preserve generated configuration documentation during the lint rollout."
    )]
    pub options: JsonSerializerOptions,
}

/// Options for the JsonSerializer.
#[configurable_component]
#[derive(Debug, Clone, Default)]
// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::doc_markdown,
    reason = "Preserve generated configuration documentation during the lint rollout."
)]
pub struct JsonSerializerOptions {
    /// Whether to use pretty JSON formatting.
    #[serde(default)]
    pub pretty: bool,

    /// Controls how binary data in string values is encoded.
    ///
    /// String values can hold arbitrary bytes that are not valid UTF-8, such as binary WebSocket
    /// frames. This option applies to every string value in log and trace events, including
    /// fields such as `host` and `source_type`, not only to those that hold binary data. Object
    /// keys, timestamps, and metric events are not affected, and neither are fields that a sink
    /// writes outside the encoded event, such as the Splunk HEC `fields`.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub bytes_format: JsonBytesFormat,
}

/// How the `JsonSerializer` encodes binary data in string values.
#[configurable_component]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JsonBytesFormat {
    /// Encode strings as UTF-8, replacing invalid UTF-8 sequences with the
    /// [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD]. Bytes that are not valid UTF-8 cannot be
    /// recovered.
    ///
    /// [U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
    #[default]
    LossyUtf8,

    /// Encode strings as [standard padded base64][rfc4648], the same output as the VRL
    /// `encode_base64` function with its default options. All bytes are preserved, so consumers
    /// must base64-decode every string value. Encoded strings are about a third larger.
    ///
    /// [rfc4648]: https://datatracker.ietf.org/doc/html/rfc4648#section-4
    Base64,
}

impl JsonSerializerConfig {
    /// Creates a new `JsonSerializerConfig`.
    #[must_use]
    pub const fn new(metric_tag_values: MetricTagValues, options: JsonSerializerOptions) -> Self {
        Self {
            metric_tag_values,
            options,
        }
    }

    /// Build the `JsonSerializer` from this configuration.
    #[must_use]
    pub fn build(&self) -> JsonSerializer {
        JsonSerializer::new(self.metric_tag_values, self.options.clone())
    }

    /// The data type of events that are accepted by `JsonSerializer`.
    #[must_use]
    pub fn input_type(&self) -> DataType {
        DataType::all_bits()
    }

    /// The schema required by the serializer.
    #[must_use]
    pub fn schema_requirement(&self) -> schema::Requirement {
        // While technically we support `Value` variants that can't be losslessly serialized to
        // JSON, we don't want to enforce that limitation to users yet.
        schema::Requirement::empty()
    }
}

/// Serializer that converts an `Event` to bytes using the JSON format.
#[derive(Debug, Clone)]
pub struct JsonSerializer {
    metric_tag_values: MetricTagValues,
    options: JsonSerializerOptions,
}

impl JsonSerializer {
    /// Creates a new `JsonSerializer`.
    #[must_use]
    pub const fn new(metric_tag_values: MetricTagValues, options: JsonSerializerOptions) -> Self {
        Self {
            metric_tag_values,
            options,
        }
    }

    /// Encode event and represent it as JSON value.
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_errors_doc,
        reason = "The codec API error documentation needs a separate audit."
    )]
    pub fn to_json_value(&self, event: Event) -> Result<serde_json::Value, vector_common::Error> {
        let format = self.options.bytes_format;
        match event {
            Event::Log(log) => serde_json::to_value(EncodedValue::new(log.value(), format)),
            Event::Metric(metric) => serde_json::to_value(&metric),
            Event::Trace(trace) => serde_json::to_value(EncodedValue::new(trace.value(), format)),
        }
        .map_err(|e| e.to_string().into())
    }
}

/// Serializes a log or trace event value with the configured [`JsonBytesFormat`].
///
/// `Value`'s own `Serialize` impl (from VRL) always writes bytes as lossy UTF-8 and serializes
/// nested values with itself, with no way to pass an option down. So for `base64` this walks
/// objects and arrays itself and writes each byte value straight into the output, instead of
/// copying the event.
struct EncodedValue<'a> {
    value: &'a Value,
    format: JsonBytesFormat,
}

impl<'a> EncodedValue<'a> {
    const fn new(value: &'a Value, format: JsonBytesFormat) -> Self {
        Self { value, format }
    }
}

impl Serialize for EncodedValue<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.format {
            JsonBytesFormat::LossyUtf8 => self.value.serialize(serializer),
            JsonBytesFormat::Base64 => match self.value {
                Value::Bytes(bytes) => {
                    serializer.collect_str(&Base64Display::new(bytes, &STANDARD))
                }
                Value::Object(map) => serializer.collect_map(
                    map.iter()
                        .map(|(key, value)| (key, EncodedValue::new(value, self.format))),
                ),
                Value::Array(array) => serializer.collect_seq(
                    array
                        .iter()
                        .map(|value| EncodedValue::new(value, self.format)),
                ),
                value => value.serialize(serializer),
            },
        }
    }
}

impl Encoder<Event> for JsonSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let writer = buffer.writer();
        let format = self.options.bytes_format;
        if self.options.pretty {
            match event {
                Event::Log(log) => {
                    serde_json::to_writer_pretty(writer, &EncodedValue::new(log.value(), format))
                }
                Event::Metric(mut metric) => {
                    if self.metric_tag_values == MetricTagValues::Single {
                        metric.reduce_tags_to_single();
                    }
                    serde_json::to_writer_pretty(writer, &metric)
                }
                Event::Trace(trace) => {
                    serde_json::to_writer_pretty(writer, &EncodedValue::new(trace.value(), format))
                }
            }
        } else {
            match event {
                Event::Log(log) => {
                    serde_json::to_writer(writer, &EncodedValue::new(log.value(), format))
                }
                Event::Metric(mut metric) => {
                    if self.metric_tag_values == MetricTagValues::Single {
                        metric.reduce_tags_to_single();
                    }
                    serde_json::to_writer(writer, &metric)
                }
                Event::Trace(trace) => {
                    serde_json::to_writer(writer, &EncodedValue::new(trace.value(), format))
                }
            }
        }
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};
    use chrono::{TimeZone, Timelike, Utc};
    use vector_core::{
        event::{LogEvent, Metric, MetricKind, MetricValue, StatisticKind, TraceEvent, Value},
        metric_tags,
    };
    use vrl::btreemap;

    use super::*;

    #[test]
    fn serialize_json_log() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "x" => Value::from("23"),
            "z" => Value::from(25),
            "a" => Value::from("0"),
        }));
        let bytes = serialize(JsonSerializerConfig::default(), event);

        assert_eq!(bytes, r#"{"a":"0","x":"23","z":25}"#);
    }

    #[test]
    fn serialize_json_metric_counter() {
        let event = Event::Metric(
            Metric::new(
                "foos",
                MetricKind::Incremental,
                MetricValue::Counter { value: 100.0 },
            )
            .with_namespace(Some("vector"))
            .with_tags(Some(metric_tags!(
                "key2" => "value2",
                "key1" => "value1",
                "Key3" => "Value3",
            )))
            .with_timestamp(Some(
                Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                    .single()
                    .and_then(|t| t.with_nanosecond(11))
                    .expect("invalid timestamp"),
            )),
        );

        let bytes = serialize(JsonSerializerConfig::default(), event);

        assert_eq!(
            bytes,
            r#"{"name":"foos","namespace":"vector","tags":{"Key3":"Value3","key1":"value1","key2":"value2"},"timestamp":"2018-11-14T08:09:10.000000011Z","kind":"incremental","counter":{"value":100.0}}"#
        );
    }

    #[test]
    fn serialize_json_metric_set() {
        let event = Event::Metric(Metric::new(
            "users",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["bob".into()].into_iter().collect(),
            },
        ));

        let bytes = serialize(JsonSerializerConfig::default(), event);

        assert_eq!(
            bytes,
            r#"{"name":"users","kind":"incremental","set":{"values":["bob"]}}"#
        );
    }

    #[test]
    fn serialize_json_metric_histogram_without_timestamp() {
        let event = Event::Metric(Metric::new(
            "glork",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vector_core::samples![10.0 => 1],
                statistic: StatisticKind::Histogram,
            },
        ));

        let bytes = serialize(JsonSerializerConfig::default(), event);

        assert_eq!(
            bytes,
            r#"{"name":"glork","kind":"incremental","distribution":{"samples":[{"value":10.0,"rate":1}],"statistic":"histogram"}}"#
        );
    }

    #[test]
    fn serialize_equals_to_json_value() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "foo" => Value::from("bar")
        }));
        let mut serializer = JsonSerializerConfig::default().build();
        let mut bytes = BytesMut::new();

        serializer.encode(event.clone(), &mut bytes).unwrap();

        let json = serializer.to_json_value(event).unwrap();

        assert_eq!(bytes.freeze(), serde_json::to_string(&json).unwrap());
    }

    #[test]
    fn serialize_metric_tags_full() {
        let bytes = serialize(
            JsonSerializerConfig {
                metric_tag_values: MetricTagValues::Full,
                options: JsonSerializerOptions::default(),
            },
            metric2(),
        );

        assert_eq!(
            bytes,
            r#"{"name":"counter","tags":{"a":["first",null,"second"]},"kind":"incremental","counter":{"value":1.0}}"#
        );
    }

    #[test]
    fn serialize_metric_tags_single() {
        let bytes = serialize(
            JsonSerializerConfig {
                metric_tag_values: MetricTagValues::Single,
                options: JsonSerializerOptions::default(),
            },
            metric2(),
        );

        assert_eq!(
            bytes,
            r#"{"name":"counter","tags":{"a":"second"},"kind":"incremental","counter":{"value":1.0}}"#
        );
    }

    #[test]
    fn serialize_invalid_utf8_lossy_by_default() {
        let bytes = serialize(JsonSerializerConfig::default(), binary_log());

        assert_eq!(
            bytes,
            "{\"message\":\"\u{fffd}\\u0000\\u0004>\u{400}\u{fffd}\"}"
        );
    }

    #[test]
    fn default_output_matches_plain_serde_json() {
        let log = LogEvent::from(btreemap! {
            "message" => Value::Bytes(Bytes::from_static(BINARY)),
            "nested" => Value::from(btreemap! {
                "array" => Value::from(vec![Value::Bytes(Bytes::from_static(BINARY))]),
            }),
        });
        let trace = TraceEvent::from(log.clone());
        let pretty = JsonSerializerConfig::new(
            MetricTagValues::default(),
            JsonSerializerOptions {
                pretty: true,
                ..Default::default()
            },
        );

        let serializer = JsonSerializerConfig::default().build();
        assert_eq!(
            serialize(JsonSerializerConfig::default(), Event::Log(log.clone())),
            serde_json::to_vec(&log).unwrap()
        );
        assert_eq!(
            serialize(JsonSerializerConfig::default(), Event::Trace(trace.clone())),
            serde_json::to_vec(&trace).unwrap()
        );
        assert_eq!(
            serialize(pretty.clone(), Event::Log(log.clone())),
            serde_json::to_vec_pretty(&log).unwrap()
        );
        assert_eq!(
            serialize(pretty, Event::Trace(trace.clone())),
            serde_json::to_vec_pretty(&trace).unwrap()
        );
        assert_eq!(
            serializer.to_json_value(Event::Log(log.clone())).unwrap(),
            serde_json::to_value(&log).unwrap()
        );
        assert_eq!(
            serializer
                .to_json_value(Event::Trace(trace.clone()))
                .unwrap(),
            serde_json::to_value(&trace).unwrap()
        );
    }

    /// Bytes that are not valid UTF-8: `0xd3` is not followed by a continuation byte, and `0xff`
    /// never occurs in UTF-8.
    const BINARY: &[u8] = &[0xd3, 0x00, 0x04, 0x3e, 0xd0, 0x80, 0xff];

    fn binary_log() -> Event {
        Event::Log(LogEvent::from(btreemap! {
            "message" => Value::Bytes(Bytes::from_static(BINARY)),
        }))
    }

    fn metric2() -> Event {
        Event::Metric(
            Metric::new(
                "counter",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            )
            .with_tags(Some(metric_tags! (
                "a" => "first",
                "a" => None,
                "a" => "second",
            ))),
        )
    }

    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::needless_pass_by_value,
        reason = "Keep the existing owned-argument API during the lint rollout."
    )]
    fn serialize(config: JsonSerializerConfig, input: Event) -> Bytes {
        let mut buffer = BytesMut::new();
        config.build().encode(input, &mut buffer).unwrap();
        buffer.freeze()
    }

    mod pretty_json {
        use bytes::{Bytes, BytesMut};
        use chrono::{TimeZone, Timelike, Utc};
        use vector_core::{
            event::{LogEvent, Metric, MetricKind, MetricValue, StatisticKind, Value},
            metric_tags,
        };
        use vrl::btreemap;

        use super::*;

        fn get_pretty_json_config() -> JsonSerializerConfig {
            JsonSerializerConfig {
                options: JsonSerializerOptions {
                    pretty: true,
                    ..Default::default()
                },
                ..Default::default()
            }
        }

        #[test]
        fn serialize_json_log() {
            let event = Event::Log(LogEvent::from(
                btreemap! {"x" => Value::from("23"),"z" => Value::from(25),"a" => Value::from("0"),},
            ));
            let bytes = serialize(get_pretty_json_config(), event);
            assert_eq!(
                bytes,
                r#"{
  "a": "0",
  "x": "23",
  "z": 25
}"#
            );
        }
        #[test]
        fn serialize_json_metric_counter() {
            let event = Event::Metric(
                Metric::new(
                    "foos",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 100.0 },
                )
                .with_namespace(Some("vector"))
                .with_tags(Some(
                    metric_tags!("key2" => "value2","key1" => "value1","Key3" => "Value3",),
                ))
                .with_timestamp(Some(
                    Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                        .single()
                        .and_then(|t| t.with_nanosecond(11))
                        .expect("invalid timestamp"),
                )),
            );
            let bytes = serialize(get_pretty_json_config(), event);
            assert_eq!(
                bytes,
                r#"{
  "name": "foos",
  "namespace": "vector",
  "tags": {
    "Key3": "Value3",
    "key1": "value1",
    "key2": "value2"
  },
  "timestamp": "2018-11-14T08:09:10.000000011Z",
  "kind": "incremental",
  "counter": {
    "value": 100.0
  }
}"#
            );
        }
        #[test]
        fn serialize_json_metric_set() {
            let event = Event::Metric(Metric::new(
                "users",
                MetricKind::Incremental,
                MetricValue::Set {
                    values: vec!["bob".into()].into_iter().collect(),
                },
            ));
            let bytes = serialize(get_pretty_json_config(), event);
            assert_eq!(
                bytes,
                r#"{
  "name": "users",
  "kind": "incremental",
  "set": {
    "values": [
      "bob"
    ]
  }
}"#
            );
        }
        #[test]
        fn serialize_json_metric_histogram_without_timestamp() {
            let event = Event::Metric(Metric::new(
                "glork",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: vector_core::samples![10.0 => 1],
                    statistic: StatisticKind::Histogram,
                },
            ));
            let bytes = serialize(get_pretty_json_config(), event);
            assert_eq!(
                bytes,
                r#"{
  "name": "glork",
  "kind": "incremental",
  "distribution": {
    "samples": [
      {
        "value": 10.0,
        "rate": 1
      }
    ],
    "statistic": "histogram"
  }
}"#
            );
        }
        #[test]
        fn serialize_equals_to_json_value() {
            let event = Event::Log(LogEvent::from(btreemap! {"foo" => Value::from("bar")}));
            let mut serializer = get_pretty_json_config().build();
            let mut bytes = BytesMut::new();
            serializer.encode(event.clone(), &mut bytes).unwrap();
            let json = serializer.to_json_value(event).unwrap();
            assert_eq!(bytes.freeze(), serde_json::to_string_pretty(&json).unwrap());
        }
        #[test]
        fn serialize_metric_tags_full() {
            let bytes = serialize(
                JsonSerializerConfig {
                    metric_tag_values: MetricTagValues::Full,
                    options: JsonSerializerOptions {
                        pretty: true,
                        ..Default::default()
                    },
                },
                metric2(),
            );
            assert_eq!(
                bytes,
                r#"{
  "name": "counter",
  "tags": {
    "a": [
      "first",
      null,
      "second"
    ]
  },
  "kind": "incremental",
  "counter": {
    "value": 1.0
  }
}"#
            );
        }
        #[test]
        fn serialize_metric_tags_single() {
            let bytes = serialize(
                JsonSerializerConfig {
                    metric_tag_values: MetricTagValues::Single,
                    options: JsonSerializerOptions {
                        pretty: true,
                        ..Default::default()
                    },
                },
                metric2(),
            );
            assert_eq!(
                bytes,
                r#"{
  "name": "counter",
  "tags": {
    "a": "second"
  },
  "kind": "incremental",
  "counter": {
    "value": 1.0
  }
}"#
            );
        }
        fn metric2() -> Event {
            Event::Metric(
                Metric::new(
                    "counter",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_tags(Some(
                    metric_tags! ("a" => "first","a" => None,"a" => "second",),
                )),
            )
        }
        // https://github.com/vectordotdev/vector/issues/23659
        #[allow(
            clippy::needless_pass_by_value,
            reason = "Keep the existing owned-argument API during the lint rollout."
        )]
        fn serialize(config: JsonSerializerConfig, input: Event) -> Bytes {
            let mut buffer = BytesMut::new();
            config.build().encode(input, &mut buffer).unwrap();
            buffer.freeze()
        }
    }

    mod base64_bytes {
        use base64::Engine;
        use vector_core::event::ObjectMap;

        use super::*;
        use crate::encoding::{EncodingConfig, SerializerConfig};

        fn base64_config(pretty: bool) -> JsonSerializerConfig {
            JsonSerializerConfig {
                options: JsonSerializerOptions {
                    pretty,
                    bytes_format: JsonBytesFormat::Base64,
                },
                ..Default::default()
            }
        }

        #[test]
        fn serialize_bytes_field() {
            let bytes = serialize(base64_config(false), binary_log());

            assert_eq!(bytes, r#"{"message":"0wAEPtCA/w=="}"#);
        }

        #[test]
        fn serialize_bytes_root() {
            // With the Vector log namespace, sources such as `websocket` emit the raw bytes as
            // the event itself.
            let event = Event::Log(LogEvent::from(Value::Bytes(Bytes::from_static(BINARY))));
            let bytes = serialize(base64_config(false), event);

            assert_eq!(bytes, r#""0wAEPtCA/w==""#);
        }

        #[test]
        fn serialize_nested_bytes() {
            let event = Event::Log(LogEvent::from(btreemap! {
                "array" => Value::from(vec![
                    Value::from("abc"),
                    Value::from(btreemap! { "inner" => Value::from("abc") }),
                ]),
                "object" => Value::from(btreemap! {
                    "array" => Value::from(vec![Value::from("abc")]),
                    "inner" => Value::from("abc"),
                }),
            }));
            let bytes = serialize(base64_config(false), event);

            assert_eq!(
                bytes,
                r#"{"array":["YWJj",{"inner":"YWJj"}],"object":{"array":["YWJj"],"inner":"YWJj"}}"#
            );
        }

        #[test]
        fn serialize_empty_bytes() {
            let event = Event::Log(LogEvent::from(btreemap! {
                "message" => Value::from(""),
            }));
            let bytes = serialize(base64_config(false), event);

            assert_eq!(bytes, r#"{"message":""}"#);
        }

        #[test]
        fn round_trip_every_byte_value() {
            let binary: Vec<u8> = (0..=u8::MAX).collect();
            let event = Event::Log(LogEvent::from(btreemap! {
                "message" => Value::Bytes(Bytes::from(binary.clone())),
            }));
            let bytes = serialize(base64_config(false), event);

            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let decoded = STANDARD.decode(json["message"].as_str().unwrap()).unwrap();
            assert_eq!(decoded, binary);
        }

        #[test]
        fn serialize_other_values_unchanged() {
            let event = Event::Log(LogEvent::from(btreemap! {
                "boolean" => Value::from(true),
                "empty_array" => Value::Array(Vec::new()),
                "empty_object" => Value::Object(ObjectMap::new()),
                "float" => Value::from(1.5),
                "integer" => Value::from(25),
                "null" => Value::Null,
                "timestamp" => Value::from(Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10).unwrap()),
            }));
            let bytes = serialize(base64_config(false), event);

            assert_eq!(
                bytes,
                r#"{"boolean":true,"empty_array":[],"empty_object":{},"float":1.5,"integer":25,"null":null,"timestamp":"2018-11-14T08:09:10Z"}"#
            );
        }

        #[test]
        fn serialize_trace_bytes() {
            let event = Event::Trace(TraceEvent::from(LogEvent::from(btreemap! {
                "span_id" => Value::Bytes(Bytes::from_static(BINARY)),
            })));
            let bytes = serialize(base64_config(false), event);

            assert_eq!(bytes, r#"{"span_id":"0wAEPtCA/w=="}"#);
        }

        #[test]
        fn serialize_metric_unchanged() {
            assert_eq!(
                serialize(base64_config(false), metric2()),
                serialize(JsonSerializerConfig::default(), metric2())
            );
        }

        #[test]
        fn serialize_pretty() {
            let event = Event::Log(LogEvent::from(btreemap! {
                "array" => Value::from(vec![Value::Bytes(Bytes::from_static(BINARY))]),
                "message" => Value::Bytes(Bytes::from_static(BINARY)),
            }));
            let bytes = serialize(base64_config(true), event);

            assert_eq!(
                bytes,
                r#"{
  "array": [
    "0wAEPtCA/w=="
  ],
  "message": "0wAEPtCA/w=="
}"#
            );
        }

        #[test]
        fn serialize_equals_to_json_value() {
            let mut serializer = base64_config(false).build();
            let mut bytes = BytesMut::new();

            serializer.encode(binary_log(), &mut bytes).unwrap();

            let json = serializer.to_json_value(binary_log()).unwrap();

            assert_eq!(bytes.freeze(), serde_json::to_string(&json).unwrap());
        }

        #[test]
        fn deserialize_encoding_config() {
            let config: EncodingConfig =
                serde_json::from_str(r#"{"codec":"json","json":{"bytes_format":"base64"}}"#)
                    .unwrap();
            let mut serializer = config.build().unwrap();
            let mut bytes = BytesMut::new();
            serializer.encode(binary_log(), &mut bytes).unwrap();
            assert_eq!(bytes.freeze(), r#"{"message":"0wAEPtCA/w=="}"#);

            let config: EncodingConfig = serde_json::from_str(r#"{"codec":"json"}"#).unwrap();
            assert!(matches!(
                config.config(),
                SerializerConfig::Json(json) if json.options.bytes_format == JsonBytesFormat::LossyUtf8
            ));

            let error = serde_json::from_str::<EncodingConfig>(
                r#"{"codec":"json","json":{"bytes_format":"hex"}}"#,
            )
            .unwrap_err();
            assert!(error.to_string().contains("hex"), "{error}");
        }
    }
}
