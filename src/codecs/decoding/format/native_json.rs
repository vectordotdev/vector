use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use value::Kind;

use super::Deserializer;
use crate::{event::Event, schema};

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

/// Deserializer that builds `Event`s from a byte frame containing JSON.
#[derive(Debug, Clone, Default)]
pub struct NativeJsonDeserializer;

impl Deserializer for NativeJsonDeserializer {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        // It's common to receive empty frames when parsing NDJSON, since it
        // allows multiple empty newlines. We proceed without a warning here.
        if bytes.is_empty() {
            return Ok(smallvec![]);
        }

        // TODO: do we want to parse arrays?
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
