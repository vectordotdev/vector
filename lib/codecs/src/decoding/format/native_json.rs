use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use value::Kind;

use super::Deserializer;
use vector_core::{event::Event, schema};

/// Config used to build a `NativeJsonDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NativeJsonDeserializerConfig;

impl NativeJsonDeserializerConfig {
    /// Build the `NativeJsonDeserializer` from this configuration.
    pub const fn build(&self) -> NativeJsonDeserializer {
        NativeJsonDeserializer
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self) -> schema::Definition {
        schema::Definition::empty().unknown_fields(Kind::json())
    }
}

/// Deserializer that builds `Event`s from a byte frame containing Vector's native JSON
/// representation.
#[derive(Debug, Clone, Default)]
pub struct NativeJsonDeserializer;

impl Deserializer for NativeJsonDeserializer {
    fn parse(&self, bytes: Bytes) -> vector_core::Result<SmallVec<[Event; 1]>> {
        // It's common to receive empty frames when parsing NDJSON, since it
        // allows multiple empty newlines. We proceed without a warning here.
        if bytes.is_empty() {
            return Ok(smallvec![]);
        }

        let json: serde_json::Value = serde_json::from_slice(&bytes)
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
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_top_level_arrays() {
        let config = NativeJsonDeserializerConfig;
        let deserializer = config.build();

        let json1 = json!({"a": "b", "c": "d"});
        let json2 = json!({"foo": "bar", "baz": "quux"});
        let json_array = json!([{ "log": json1 }, { "log": json2 }]);
        let input = Bytes::from(serde_json::to_vec(&json_array).unwrap());

        let events = deserializer.parse(input).unwrap();

        let event1 = Event::try_from(json1).unwrap();
        let event2 = Event::try_from(json2).unwrap();
        let expected: SmallVec<[Event; 1]> = smallvec![event1, event2];
        assert_eq!(events, expected);
    }
}
