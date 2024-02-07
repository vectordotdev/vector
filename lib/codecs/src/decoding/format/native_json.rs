use bytes::Bytes;
use derivative::Derivative;
use smallvec::{smallvec, SmallVec};
use vector_config::configurable_component;
use vector_core::{config::DataType, event::Event, schema};
use vrl::value::kind::Collection;
use vrl::value::Kind;

use super::{default_lossy, Deserializer};
use vector_core::config::LogNamespace;

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
        DataType::all()
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
    /// Determines whether or not to replace invalid UTF-8 sequences instead of failing.
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
        .map_err(|error| format!("Error parsing JSON: {:?}", error))?;

        let events = match json {
            serde_json::Value::Array(values) => values
                .into_iter()
                .map(serde_json::from_value)
                .collect::<Result<SmallVec<[Event; 1]>, _>>()?,
            _ => smallvec![serde_json::from_value(json)?],
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
}
