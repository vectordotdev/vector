use std::collections::BTreeMap;
use std::fs;

use bytes::Bytes;
use chrono::Utc;
use lookup::PathPrefix;
use ordered_float::NotNan;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor, ReflectMessage};
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
    /// Path to desc file
    desc_file: String,

    /// message type. e.g package.message
    message_type: String,
}

impl ProtobufDeserializerConfig {
    /// Build the `ProtobufDeserializer` from this configuration.
    pub fn build(&self) -> ProtobufDeserializer {
        Into::<ProtobufDeserializer>::into(self)
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
                    schema::Definition::empty_legacy_namespace().unknown_fields(Kind::json()); // TODO: create kind::protobuf?

                if let Some(timestamp_key) = log_schema().timestamp_key() {
                    definition = definition.try_with_field(
                        timestamp_key,
                        // The PROTOBUF decoder will try to insert a new `timestamp`-type value into the
                        // "timestamp_key" field, but only if that field doesn't already exist.
                        Kind::json().or_timestamp(), // TODO: create kind::protobuf?
                        Some("timestamp"),
                    );
                }
                definition
            }
            LogNamespace::Vector => {
                schema::Definition::new_with_default_metadata(Kind::json(), [log_namespace])
                // TODO: create kind::protobuf?
            }
        }
    }
}

impl ProtobufDeserializerConfig {
    /// Creates a new `ProtobufDeserializerConfig`.
    pub fn new() -> Self {
        Default::default()
    }
}

/// Deserializer that builds `Event`s from a byte frame containing PROTOBUF.
#[derive(Debug, Clone)]
pub struct ProtobufDeserializer {
    message_descriptor: MessageDescriptor,
}

impl ProtobufDeserializer {
    /// Creates a new `ProtobufDeserializer`.
    pub fn new(desc_file: String, message_type: String) -> Self {
        // TODO: handle 'expect' in a better way
        let b = fs::read(desc_file.clone())
            .expect(&format!("Failed to open protobuf desc file '{desc_file}'"));
        let pool = DescriptorPool::decode(b.as_slice())
            .expect(&format!("Failed to parse protobuf desc file '{desc_file}'"));
        Self {
            message_descriptor: pool.get_message_by_name(&message_type).expect(&format!(
                "The message type '{message_type}' could not be found in '{desc_file}'"
            )),
        }
    }
}

impl Deserializer for ProtobufDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        if bytes.is_empty() {
            return Ok(smallvec![]);
        }

        let dynamic_message = DynamicMessage::decode(self.message_descriptor.clone(), bytes)
            .map_err(|error| format!("Error parsing PROTOBUF: {:?}", error))?;

        let proto_vrl = to_vrl(
            prost_reflect::Value::Message(dynamic_message),
            &prost_reflect::Kind::Message(self.message_descriptor.clone()),
        )?;
        let mut event = Event::Log(LogEvent::from(proto_vrl));
        let event = match log_namespace {
            LogNamespace::Vector => event,
            LogNamespace::Legacy => {
                let timestamp = Utc::now();

                if let Some(timestamp_key) = log_schema().timestamp_key() {
                    let log = event.as_mut_log();
                    if !log.contains((PathPrefix::Event, timestamp_key)) {
                        log.insert((PathPrefix::Event, timestamp_key), timestamp);
                    }
                }

                event
            }
        };

        Ok(smallvec![event])
    }
}

impl From<&ProtobufDeserializerConfig> for ProtobufDeserializer {
    fn from(config: &ProtobufDeserializerConfig) -> Self {
        Self::new(config.desc_file.clone(), config.message_type.clone())
    }
}

fn to_vrl(
    prost_reflect_value: prost_reflect::Value,
    kind: &prost_reflect::Kind,
) -> vector_common::Result<vrl::value::Value> {
    let v = match prost_reflect_value {
        prost_reflect::Value::Bool(v) => vrl::value::Value::from(v),
        prost_reflect::Value::I32(v) => vrl::value::Value::from(v),
        prost_reflect::Value::I64(v) => vrl::value::Value::from(v),
        prost_reflect::Value::U32(v) => vrl::value::Value::from(v),
        prost_reflect::Value::U64(v) => vrl::value::Value::from(v),
        prost_reflect::Value::F32(v) => vrl::value::Value::Float(
            NotNan::new(f64::from(v)).map_err(|_e| format!("Float number cannot be Nan"))?,
        ),
        prost_reflect::Value::F64(v) => vrl::value::Value::Float(
            NotNan::new(v).map_err(|_e| format!("F64 number cannot be Nan"))?,
        ),
        prost_reflect::Value::String(v) => vrl::value::Value::from(v),
        prost_reflect::Value::Bytes(v) => vrl::value::Value::from(v),
        prost_reflect::Value::EnumNumber(v) => {
            let enum_desc = kind.as_enum().unwrap();
            vrl::value::Value::from(
                enum_desc
                    .get_value(v)
                    .ok_or_else(|| format!("The number {} cannot be in {}", v, enum_desc.name()))?
                    .name(),
            )
        }
        prost_reflect::Value::Message(mut v) => {
            let mut obj_map = BTreeMap::new();
            for field_desc in v.descriptor().fields() {
                let field = v.get_field_mut(&field_desc);
                let mut taken_value = prost_reflect::Value::Bool(false);
                std::mem::swap(&mut taken_value, field);
                let out = to_vrl(taken_value, &field_desc.kind())?;
                obj_map.insert(field_desc.name().to_string(), out);
            }
            vrl::value::Value::from(obj_map)
        }
        prost_reflect::Value::List(v) => {
            let vec = v
                .into_iter()
                .map(|o| to_vrl(o, &kind))
                .collect::<Result<Vec<_>, vector_common::Error>>()?;
            vrl::value::Value::from(vec)
        }
        prost_reflect::Value::Map(v) => {
            let message_desc = kind.as_message().unwrap();
            vrl::value::Value::from(
                v.into_iter()
                    // TODO: handle unwrap
                    .map(|kv| {
                        (
                            kv.0.as_str().unwrap().to_string(),
                            to_vrl(kv.1, &message_desc.map_entry_value_field().kind()).unwrap(),
                        )
                    })
                    .collect::<BTreeMap<String, _>>(),
            )
        }
    };
    Ok(v)
}

#[cfg(test)]
mod tests {
    // TODO: add test for bad file path & invalid message_type

    use std::fs;
    use vector_core::config::log_schema;

    use super::*;

    #[test]
    fn deserialize_protobuf() {
        let protobuf_bin_message_path = "tests/data/protobuf_decoding/person_someone.pb";
        let protobuf_desc_path = "tests/data/protobuf_decoding/test_protobuf.desc";
        let message_type = "test_protobuf.Person";
        let validate_log = |log: &LogEvent| {
            assert_eq!(log["name"], "someone".into());
        };

        parse_and_validate(
            protobuf_bin_message_path,
            protobuf_desc_path,
            message_type,
            validate_log,
        );
    }

    #[test]
    fn deserialize_protobuf3() {
        let protobuf_bin_message_path = "tests/data/protobuf_decoding/person_someone3.pb";
        let protobuf_desc_path = "tests/data/protobuf_decoding/test_protobuf3.desc";
        let message_type = "test_protobuf3.Person";
        let validate_log = |log: &LogEvent| {
            assert_eq!(log["name"], "someone".into());
            assert_eq!(
                log["data"].as_object().unwrap()["data_phone"],
                "HOME".into()
            );
        };

        parse_and_validate(
            protobuf_bin_message_path,
            protobuf_desc_path,
            message_type,
            validate_log,
        );
    }

    fn parse_and_validate(
        protobuf_bin_message_path: &str,
        protobuf_desc_path: &str,
        message_type: &str,
        validate_log: fn(&LogEvent),
    ) {
        let protobuf_message = fs::read_to_string(protobuf_bin_message_path).unwrap();
        let input = Bytes::from(protobuf_message);
        let deserializer =
            ProtobufDeserializer::new(protobuf_desc_path.to_string(), message_type.to_string());

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            let mut events = events.into_iter();

            {
                let event = events.next().unwrap();
                let log = event.as_log();
                validate_log(log);
                assert_eq!(
                    log.get((
                        lookup::PathPrefix::Event,
                        log_schema().timestamp_key().unwrap()
                    ))
                    .is_some(),
                    namespace == LogNamespace::Legacy
                );
            }

            assert_eq!(events.next(), None);
        }
    }

    #[test]
    fn deserialize_skip_empty() {
        let input = Bytes::from("");
        let deserializer = ProtobufDeserializer::new(
            "tests/data/protobuf_decoding/test_protobuf.desc".to_string(),
            "test_protobuf.Person".to_string(),
        );

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            assert!(events.is_empty());
        }
    }

    #[test]
    fn deserialize_error_invalid_protobuf() {
        let input = Bytes::from("{ foo");
        let deserializer = ProtobufDeserializer::new(
            "tests/data/protobuf_decoding/test_protobuf.desc".to_string(),
            "test_protobuf.Person".to_string(),
        );

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            assert!(deserializer.parse(input.clone(), namespace).is_err());
        }
    }
}
