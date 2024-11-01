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
        let message_descriptor = vrl::protobuf::get_message_descriptor(
            &config.protobuf.desc_file,
            &config.protobuf.message_type,
        )?;
        Ok(Self::new(message_descriptor))
    }
}

#[cfg(test)]
mod tests {
    // TODO: add test for bad file path & invalid message_type

    use std::path::PathBuf;
    use std::{env, fs};
    use vector_core::config::log_schema;

    use super::*;

    fn test_data_dir() -> PathBuf {
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("tests/data/protobuf")
    }

    fn parse_and_validate(
        protobuf_bin_message: String,
        protobuf_desc_path: PathBuf,
        message_type: &str,
        validate_log: fn(&LogEvent),
    ) {
        let input = Bytes::from(protobuf_bin_message);
        let message_descriptor =
            vrl::protobuf::get_message_descriptor(&protobuf_desc_path, message_type).unwrap();
        let deserializer = ProtobufDeserializer::new(message_descriptor);

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            let mut events = events.into_iter();

            {
                let event = events.next().unwrap();
                let log = event.as_log();
                validate_log(log);
                assert_eq!(
                    log.get(log_schema().timestamp_key_target_path().unwrap())
                        .is_some(),
                    namespace == LogNamespace::Legacy
                );
            }

            assert_eq!(events.next(), None);
        }
    }

    #[test]
    fn deserialize_protobuf() {
        let protobuf_bin_message_path = test_data_dir().join("pbs/person_someone.pb");
        let protobuf_desc_path = test_data_dir().join("protos/test_protobuf.desc");
        let message_type = "test_protobuf.Person";
        let validate_log = |log: &LogEvent| {
            assert_eq!(log["name"], "someone".into());
            assert_eq!(
                log["phones"].as_array().unwrap()[0].as_object().unwrap()["number"]
                    .as_str()
                    .unwrap(),
                "123456"
            );
        };

        parse_and_validate(
            fs::read_to_string(protobuf_bin_message_path).unwrap(),
            protobuf_desc_path,
            message_type,
            validate_log,
        );
    }

    #[test]
    fn deserialize_protobuf3() {
        let protobuf_bin_message_path = test_data_dir().join("pbs/person_someone3.pb");
        let protobuf_desc_path = test_data_dir().join("protos/test_protobuf3.desc");
        let message_type = "test_protobuf3.Person";
        let validate_log = |log: &LogEvent| {
            assert_eq!(log["name"], "someone".into());
            assert_eq!(
                log["phones"].as_array().unwrap()[0].as_object().unwrap()["number"]
                    .as_str()
                    .unwrap(),
                "1234"
            );
            assert_eq!(
                log["data"].as_object().unwrap()["data_phone"],
                "HOME".into()
            );
        };

        parse_and_validate(
            fs::read_to_string(protobuf_bin_message_path).unwrap(),
            protobuf_desc_path,
            message_type,
            validate_log,
        );
    }

    #[test]
    fn deserialize_empty_buffer() {
        let protobuf_bin_message = "".to_string();
        let protobuf_desc_path = test_data_dir().join("protos/test_protobuf.desc");
        let message_type = "test_protobuf.Person";
        let validate_log = |log: &LogEvent| {
            // No field will be set.
            assert!(!log.contains("name"));
            assert!(!log.contains("id"));
            assert!(!log.contains("email"));
            assert!(!log.contains("phones"));
        };

        parse_and_validate(
            protobuf_bin_message,
            protobuf_desc_path,
            message_type,
            validate_log,
        );
    }

    #[test]
    fn deserialize_error_invalid_protobuf() {
        let input = Bytes::from("{ foo");
        let message_descriptor = vrl::protobuf::get_message_descriptor(
            &test_data_dir().join("protos/test_protobuf.desc"),
            "test_protobuf.Person",
        )
        .unwrap();
        let deserializer = ProtobufDeserializer::new(message_descriptor);

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            assert!(deserializer.parse(input.clone(), namespace).is_err());
        }
    }
}
