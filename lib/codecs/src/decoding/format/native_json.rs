use bytes::Bytes;
use derivative::Derivative;
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

        let decode = |value: serde_json::Value| {
            let is_protojson = value
                .as_object()
                .is_some_and(|object| object.contains_key("event"));

            if is_protojson {
                prost_reflect::DynamicMessage::deserialize(descriptor(), value)
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
        let input = Bytes::from(serde_json::to_vec(&json!({"log": legacy_log.clone()})).unwrap());

        let events = deserializer.parse(input, LogNamespace::Legacy).unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            Event::from_json_value(legacy_log, LogNamespace::Legacy).unwrap()
        );
    }

    #[test]
    fn rejects_incomplete_protojson_without_panicking() {
        let deserializer = NativeJsonDeserializerConfig::default().build();
        let malformed = [
            json!({}),
            json!({"event": {}}),
            json!({"event": {"log": {}}}),
            json!({"event": {"log": {"value": {}}}}),
            json!({"event": {"metric": {"name": "missing-value"}}}),
            json!({"event": {"metric": {"name": "missing-sketch", "sketch": {}}}}),
            json!({
                "event": {
                    "metric": {
                        "name": "mismatched-sketch",
                        "sketch": {"agentDdSketch": {"k": [1], "n": []}}
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
}
