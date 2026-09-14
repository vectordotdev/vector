use bytes::Bytes;
use derivative::Derivative;
use prost_reflect::DeserializeOptions;
use smallvec::{SmallVec, smallvec};
use vector_config::configurable_component;
use vector_core::{
    config::{DataType, LogNamespace},
    event::Event,
    schema,
};
use vrl::value::{Kind, kind::Collection};

use super::{Deserializer, default_lossy};
use crate::native_json::{descriptor, from_dynamic_message};

/// Config used to build a `NativeJsonDeserializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct NativeJsonDeserializerConfig {
    /// Vector's native JSON-specific decoding options.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub native_json: NativeJsonDeserializerOptions,
}

impl NativeJsonDeserializerConfig {
    /// Creates a new `NativeJsonDeserializerConfig`.
    pub fn new(options: NativeJsonDeserializerOptions) -> Self {
        Self {
            native_json: options,
        }
    }

    /// Build the `NativeJsonDeserializer` from this configuration.
    pub fn build(&self) -> NativeJsonDeserializer {
        NativeJsonDeserializer {
            lossy: self.native_json.lossy,
        }
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::all_bits()
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match log_namespace {
            LogNamespace::Vector => {
                schema::Definition::new_with_default_metadata(Kind::json(), [log_namespace])
            }
            LogNamespace::Legacy => schema::Definition::new_with_default_metadata(
                Kind::object(Collection::json()),
                [log_namespace],
            ),
        }
    }
}

/// Vector's native JSON-specific decoding options.
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Derivative)]
#[derivative(Default)]
pub struct NativeJsonDeserializerOptions {
    /// Determines whether to replace invalid UTF-8 sequences instead of failing.
    ///
    /// When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].
    ///
    /// [U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
    #[serde(
        default = "default_lossy",
        skip_serializing_if = "vector_core::serde::is_default"
    )]
    #[derivative(Default(value = "default_lossy()"))]
    pub lossy: bool,
}

/// Deserializer that builds `Event`s from a byte frame containing Vector's native JSON
/// representation.
#[derive(Debug, Clone, Derivative)]
#[derivative(Default)]
pub struct NativeJsonDeserializer {
    #[derivative(Default(value = "default_lossy()"))]
    lossy: bool,
}

impl Deserializer for NativeJsonDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        // LogNamespace is ignored because Vector owns the data format being consumed and as such there
        // is no need to change the fields of the event.
        _log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        // It's common to receive empty frames when parsing NDJSON, since it
        // allows multiple empty newlines. We proceed without a warning here.
        if bytes.is_empty() {
            return Ok(smallvec![]);
        }

        let json: serde_json::Value = match self.lossy {
            true => serde_json::from_str(&String::from_utf8_lossy(&bytes)),
            false => serde_json::from_slice(&bytes),
        }
        .map_err(|error| format!("Error parsing JSON: {error:?}"))?;

        // Ignore fields added by newer Vector versions so additive Protobuf changes do not make
        // older native JSON consumers reject an otherwise understandable event.
        let protojson_options = DeserializeOptions::new().deny_unknown_fields(false);
        let decode = |value: serde_json::Value| {
            let is_protojson = value
                .as_object()
                .is_some_and(|object| object.contains_key("event"));

            if is_protojson {
                prost_reflect::DynamicMessage::deserialize_with_options(
                    descriptor(),
                    value,
                    &protojson_options,
                )
                .map_err(|error| error.to_string())
                .and_then(|message| {
                    from_dynamic_message(message).map_err(|error| error.to_string())
                })
                .map_err(Into::into)
            } else {
                // The legacy format uses an externally tagged Event with `log`, `metric`, or
                // `trace` at the top level. Keeping that disjoint from the `event` envelope avoids
                // guessing when arbitrary legacy log/trace field names resemble Protobuf fields.
                serde_json::from_value(value)
                    .map_err(|error| error.to_string())
                    .map_err(Into::into)
            }
        };

        let events: SmallVec<[Event; 1]> = match json {
            serde_json::Value::Array(values) => values
                .into_iter()
                .map(decode)
                .collect::<vector_common::Result<_>>()?,
            value => smallvec![decode(value)?],
        };

        Ok(events)
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;
    use uuid::Uuid;
    use vector_core::event::{MetricValue, metric::MetricSketch};

    use super::*;

    #[test]
    fn parses_top_level_arrays() {
        let config = NativeJsonDeserializerConfig::default();
        let deserializer = config.build();

        let json1 = json!({"a": "b", "c": "d"});
        let json2 = json!({"foo": "bar", "baz": "quux"});
        let json_array = json!([{ "log": json1 }, { "log": json2 }]);
        let input = Bytes::from(serde_json::to_vec(&json_array).unwrap());

        let events = deserializer.parse(input, LogNamespace::Legacy).unwrap();

        let event1 = Event::from_json_value(json1, LogNamespace::Legacy).unwrap();
        let event2 = Event::from_json_value(json2, LogNamespace::Legacy).unwrap();
        let expected: SmallVec<[Event; 1]> = smallvec![event1, event2];
        assert_eq!(events, expected);
    }

    #[test]
    fn preserves_legacy_fields_named_like_protobuf_fields() {
        let config = NativeJsonDeserializerConfig::default();
        let deserializer = config.build();

        let legacy_log = json!({
            "fields": {"nested": true},
            "metadataFull": {},
            "value": {"rawBytes": "still a legacy field"}
        });
        let input = Bytes::from(serde_json::to_vec(&json!({"log": legacy_log})).unwrap());

        let events = deserializer.parse(input, LogNamespace::Legacy).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            Event::from_json_value(legacy_log, LogNamespace::Legacy).unwrap()
        );
    }

    #[test]
    fn ignores_additive_protojson_fields() {
        let deserializer = NativeJsonDeserializerConfig::default().build();
        let input = Bytes::from(
            serde_json::to_vec(&json!({
                "futureEnvelopeField": true,
                "event": {
                    "log": {
                        "value": {"rawBytes": "a25vd24="},
                        "futureLogField": true
                    }
                }
            }))
            .unwrap(),
        );

        let events = deserializer.parse(input, LogNamespace::Legacy).unwrap();
        assert_eq!(
            events[0].as_log().value(),
            &vector_core::event::Value::from("known")
        );

        let unknown_event =
            Bytes::from(serde_json::to_vec(&json!({"event": {"futureEvent": {}}})).unwrap());
        assert!(
            deserializer
                .parse(unknown_event, LogNamespace::Legacy)
                .is_err()
        );
    }

    #[test]
    fn rejects_malformed_protojson_without_panicking() {
        let deserializer = NativeJsonDeserializerConfig::default().build();
        let malformed = [
            json!({}),
            json!({"event": {}}),
            json!({"event": {"log": {}}}),
            json!({"event": {"log": {"value": {}}}}),
            json!({"event": {"metric": {"name": "missing-value"}}}),
            json!({
                "event": {
                    "metric": {
                        "name": "mismatched-distribution",
                        "distribution1": {"values": [1.0, 2.0], "sampleRates": [1]}
                    }
                }
            }),
            json!({
                "event": {
                    "metric": {
                        "name": "mismatched-histogram",
                        "aggregatedHistogram1": {"buckets": [1.0, 2.0], "counts": [1]}
                    }
                }
            }),
            json!({
                "event": {
                    "metric": {
                        "name": "mismatched-summary",
                        "aggregatedSummary1": {"quantiles": [0.5, 0.9], "values": [1.0]}
                    }
                }
            }),
            json!({"event": {"metric": {"name": "missing-sketch", "sketch": {}}}}),
            json!({
                "event": {
                    "metric": {
                        "name": "mismatched-sketch",
                        "sketch": {"agentDdSketch": {"k": [1], "n": []}}
                    }
                }
            }),
            json!({"event": {"metric": {"name": "invalid-kind", "kind": 99, "counter": {}}}}),
            json!({
                "event": {
                    "metric": {
                        "name": "invalid-distribution1-statistic",
                        "distribution1": {"statistic": 99}
                    }
                }
            }),
            json!({
                "event": {
                    "metric": {
                        "name": "invalid-distribution2-statistic",
                        "distribution2": {"statistic": 99}
                    }
                }
            }),
            json!({"event": {"log": {"value": {"null": 99}}}}),
            json!({
                "event": {
                    "metric": {
                        "name": "sketch-key-overflow",
                        "sketch": {"agentDdSketch": {"k": [32768], "n": [1]}}
                    }
                }
            }),
            json!({
                "event": {
                    "metric": {
                        "name": "sketch-key-underflow",
                        "sketch": {"agentDdSketch": {"k": [-32769], "n": [1]}}
                    }
                }
            }),
            json!({
                "event": {
                    "metric": {
                        "name": "sketch-count-overflow",
                        "sketch": {"agentDdSketch": {"k": [0], "n": [65536]}}
                    }
                }
            }),
            json!({
                "event": {
                    "log": {
                        "value": {"map": {}},
                        "metadataFull": {"sourceEventId": "AQ=="}
                    }
                }
            }),
            json!({"event": {"log": {"value": {"float": "NaN"}}}}),
        ];

        for value in malformed {
            let input = Bytes::from(serde_json::to_vec(&value).unwrap());
            assert!(
                deserializer.parse(input, LogNamespace::Legacy).is_err(),
                "malformed native JSON unexpectedly decoded: {value}"
            );
        }
    }

    #[test]
    fn accepts_valid_protojson_boundaries() {
        let deserializer = NativeJsonDeserializerConfig::default().build();
        let sketch = Bytes::from(
            serde_json::to_vec(&json!({
                "event": {
                    "metric": {
                        "name": "sketch-boundaries",
                        "sketch": {
                            "agentDdSketch": {
                                "count": 65536,
                                "k": [-32768, 32767],
                                "n": [1, 65535]
                            }
                        }
                    }
                }
            }))
            .unwrap(),
        );
        let mut events = deserializer.parse(sketch, LogNamespace::Legacy).unwrap();
        let metric = events.pop().unwrap().into_metric();
        let MetricValue::Sketch {
            sketch: MetricSketch::AgentDDSketch(sketch),
        } = metric.value()
        else {
            panic!("decoded metric did not contain an Agent DDSketch");
        };
        assert_eq!(
            sketch.bin_map().into_parts(),
            (vec![i16::MIN, i16::MAX], vec![1, u16::MAX])
        );

        let source_event_id = Bytes::from(
            serde_json::to_vec(&json!({
                "event": {
                    "log": {
                        "value": {"map": {}},
                        "metadataFull": {
                            "sourceEventId": "AAAAAAAAAAAAAAAAAAAAAA=="
                        }
                    }
                }
            }))
            .unwrap(),
        );
        let events = deserializer
            .parse(source_event_id, LogNamespace::Legacy)
            .unwrap();
        assert_eq!(events[0].metadata().source_event_id(), Some(Uuid::nil()));
    }
}
