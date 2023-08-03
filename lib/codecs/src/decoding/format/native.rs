use bytes::Bytes;
use prost::Message;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use vector_core::config::LogNamespace;
use vector_core::{
    config::DataType,
    event::{proto, Event, EventArray, EventContainer},
    schema,
};
use vrl::value::Kind;

use super::Deserializer;

/// Config used to build a `NativeDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NativeDeserializerConfig;

impl NativeDeserializerConfig {
    /// Build the `NativeDeserializer` from this configuration.
    pub fn build(&self) -> NativeDeserializer {
        NativeDeserializer
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::all()
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match log_namespace {
            LogNamespace::Legacy => schema::Definition::empty_legacy_namespace(),
            LogNamespace::Vector => {
                schema::Definition::new_with_default_metadata(Kind::any(), [log_namespace])
            }
        }
    }
}

/// Deserializer that builds `Event`s from a byte frame containing Vector's native protobuf format.
#[derive(Debug, Clone, Default)]
pub struct NativeDeserializer;

impl Deserializer for NativeDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        // LogNamespace is ignored because Vector owns the data format being consumed and as such there
        // is no need to change the fields of the event.
        _log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        if bytes.is_empty() {
            Ok(smallvec![])
        } else {
            let event_array = EventArray::from(proto::EventArray::decode(bytes)?);
            Ok(event_array.into_events().collect())
        }
    }
}
