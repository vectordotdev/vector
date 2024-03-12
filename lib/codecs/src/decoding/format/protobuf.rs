use std::path::PathBuf;

use bytes::Bytes;
use chrono::Utc;
use derivative::Derivative;
use prost_reflect::{DynamicMessage, MessageDescriptor};
use smallvec::{smallvec, SmallVec};
use vector_config::configurable_component;
use vector_core::event::LogEvent;
use vector_core::{
    config::{log_schema, DataType, LogNamespace},
    event::Event,
    schema,
};
use vrl::value::Kind;

use crate::common::protobuf::get_message_descriptor;

use super::Deserializer;

/// Config used to build a `ProtobufDeserializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct ProtobufDeserializerConfig {
    /// Protobuf-specific decoding options.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub protobuf: ProtobufDeserializerOptions,
}

impl ProtobufDeserializerConfig {
    /// Build the `ProtobufDeserializer` from this configuration.
    pub fn build(&self) -> vector_common::Result<ProtobufDeserializer> {
        ProtobufDeserializer::try_from(self)
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match log_namespace {
            LogNamespace::Legacy => {
                let mut definition =
                    schema::Definition::empty_legacy_namespace().unknown_fields(Kind::any());

                if let Some(timestamp_key) = log_schema().timestamp_key() {
                    definition = definition.try_with_field(
                        timestamp_key,
                        // The protobuf decoder will try to insert a new `timestamp`-type value into the
                        // "timestamp_key" field, but only if that field doesn't already exist.
                        Kind::any().or_timestamp(),
                        Some("timestamp"),
                    );
                }
                definition
            }
            LogNamespace::Vector => {
                schema::Definition::new_with_default_metadata(Kind::any(), [log_namespace])
            }
        }
    }
}

/// Protobuf-specific decoding options.
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Derivative)]
#[derivative(Default)]
pub struct ProtobufDeserializerOptions {
    /// Path to desc file
    pub desc_file: PathBuf,

    /// message type. e.g package.message
    pub message_type: String,
}

/// Deserializer that builds `Event`s from a byte frame containing protobuf.
#[derive(Debug, Clone)]
pub struct ProtobufDeserializer {
    message_descriptor: MessageDescriptor,
}

impl ProtobufDeserializer {
    /// Creates a new `ProtobufDeserializer`.
    pub fn new(message_descriptor: MessageDescriptor) -> Self {
        Self { message_descriptor }
    }
}

impl Deserializer for ProtobufDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        let dynamic_message = DynamicMessage::decode(self.message_descriptor.clone(), bytes)
            .map_err(|error| format!("Error parsing protobuf: {:?}", error))?;

        let proto_vrl =
            vrl::protobuf::proto_to_value(&prost_reflect::Value::Message(dynamic_message), None)?;
        let mut event = Event::Log(LogEvent::from(proto_vrl));
        let event = match log_namespace {
            LogNamespace::Vector => event,
            LogNamespace::Legacy => {
                let timestamp = Utc::now();
                if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
                    let log = event.as_mut_log();
                    if !log.contains(timestamp_key) {
                        log.insert(timestamp_key, timestamp);
                    }
                }
                event
            }
        };

        Ok(smallvec![event])
    }
}

impl TryFrom<&ProtobufDeserializerConfig> for ProtobufDeserializer {
    type Error = vector_common::Error;
    fn try_from(config: &ProtobufDeserializerConfig) -> vector_common::Result<Self> {
        let message_descriptor =
            get_message_descriptor(&config.protobuf.desc_file, &config.protobuf.message_type)?;
        Ok(Self::new(message_descriptor))
    }
}
