use bytes::Bytes;
use prost::Message;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

use super::Deserializer;
use crate::{
    event::{proto, Event, EventArray, EventContainer},
    schema,
};

/// Config used to build a `NativeDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NativeDeserializerConfig;

impl NativeDeserializerConfig {
    /// Build the `NativeDeserializer` from this configuration.
    pub fn build(&self) -> NativeDeserializer {
        NativeDeserializer::default()
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self) -> schema::Definition {
        schema::Definition::empty()
    }
}

/// Deserializer that builds `Event`s from a byte frame containing Vector's native protobuf format.
#[derive(Debug, Clone, Default)]
pub struct NativeDeserializer;

impl Deserializer for NativeDeserializer {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        if bytes.is_empty() {
            Ok(smallvec![])
        } else {
            let event_array = EventArray::from(proto::EventArray::decode(bytes)?);
            Ok(event_array.into_events().collect())
        }
    }
}
