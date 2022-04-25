use bytes::Bytes;
use prost::Message;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use vector_core::{
    config::DataType,
    event::{proto, Event, EventArray, EventContainer},
    schema,
};

use super::Deserializer;

/// Config used to build a `NativeDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NativeDeserializerConfig;

impl NativeDeserializerConfig {
    /// Build the `NativeDeserializer` from this configuration.
    pub fn build(&self) -> NativeDeserializer {
        NativeDeserializer::default()
    }

    /// The data type of returned events
    pub fn output_type(&self) -> DataType {
        DataType::all()
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
    fn parse(&self, bytes: Bytes) -> vector_core::Result<SmallVec<[Event; 1]>> {
        if bytes.is_empty() {
            Ok(smallvec![])
        } else {
            let event_array = EventArray::from(proto::EventArray::decode(bytes)?);
            Ok(event_array.into_events().collect())
        }
    }
}
