use super::Deserializer;
use crate::encoding::AvroSerializerOptions;
use bytes::Buf;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use vector_config::configurable_component;
use vector_core::{
    config::{DataType, LogNamespace},
    event::{Event, LogEvent},
    schema,
};
use vrl::value::Value;
use lookup::event_path;

/// Config used to build a `AvroDeserializer`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AvroDeserializerConfig {
    /// Options for the Avro deserializer.
    pub avro_options: AvroDeserializerOptions,
}

impl AvroDeserializerConfig {
    /// Creates a new `AvroDeserializerConfig`.
    pub const fn new(schema: String, strip_schema_id_prefix: bool) -> Self {
        Self {
            avro_options: AvroDeserializerOptions {
                schema,
                strip_schema_id_prefix,
            },
        }
    }

    /// Build the `AvroDeserializer` from this configuration.
    pub fn build(&self) -> AvroDeserializer {
        let schema = apache_avro::Schema::parse_str(&self.avro_options.schema)
            .map_err(|error| format!("Failed building Avro serializer: {}", error))
            .unwrap();
        AvroDeserializer {
            schema,
            strip_schema_id_prefix: self.avro_options.strip_schema_id_prefix,
        }
    }

    /// The data type of events that are accepted by `AvroDeserializer`.
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_definition(&self, _log_namespace: LogNamespace) -> schema::Definition {
        // TODO: Convert the Avro schema to a vector schema definition.
        schema::Definition::any()
    }
}

impl From<&AvroDeserializerOptions> for AvroSerializerOptions {
    fn from(value: &AvroDeserializerOptions) -> Self {
        Self {
            schema: value.schema.clone(),
        }
    }
}
/// Apache Avro serializer options.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct AvroDeserializerOptions {
    /// The Avro schema.
    #[configurable(metadata(
        docs::examples = r#"{ "type": "record", "name": "log", "fields": [{ "name": "message", "type": "string" }] }"#
    ))]
    pub schema: String,

    /// for avro datum encoded in kafka messages, the bytes are prefixed with the schema id.  Set this to true to strip the schema id prefix.
    pub strip_schema_id_prefix: bool,
}

/// Serializer that converts bytes to an `Event` using the Apache Avro format.
#[derive(Debug, Clone)]
pub struct AvroDeserializer {
    schema: apache_avro::Schema,
    strip_schema_id_prefix: bool,
}

impl AvroDeserializer {
    /// Creates a new `AvroDeserializer`.
    pub const fn new(schema: apache_avro::Schema, strip_schema_id_prefix: bool) -> Self {
        Self {
            schema,
            strip_schema_id_prefix,
        }
    }

    /// Deserializes the given bytes, which will always produce a single `LogEvent`.
    pub fn parse_single(
        &self,
        bytes: Bytes,
        _log_namespace: LogNamespace,
    ) -> vector_common::Result<LogEvent> {
        let bytes = if self.strip_schema_id_prefix {
            if bytes.len() > 4 && bytes[0] == 0 {
                bytes.slice(5..)
            } else {
                return Err(vector_common::Error::from(
                    "Expected avro datum to be prefixed with schema id",
                ));
            }
        } else {
            bytes
        };

        let value = apache_avro::from_avro_datum(&self.schema, &mut bytes.reader(), None)?;

        let apache_avro::types::Value::Record(fields) = value else {
            return Err(vector_common::Error::from("Expected an avro Record"));
        };

        let mut log = LogEvent::default();
        for (k, v) in fields {
            log.insert(event_path!(k.as_str()), try_from(v)?);
        }
        Ok(log)
    }
}

impl Deserializer for AvroDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        let log = self.parse_single(bytes, log_namespace)?;
        Ok(smallvec![log.into()])
    }
}

// can't use std::convert::TryFrom because of orphan rules
pub fn try_from(value: apache_avro::types::Value) -> vector_common::Result<vrl::value::Value> {
    // very similar to avro to json see `impl std::convert::TryFrom<apache_avro::types::Value> for serde_json::Value`
    // LogEvent has native support for bytes, so it is used for Bytes and Fixed
    match value {
        apache_avro::types::Value::Array(array) => {
            let mut vector = Vec::new();
            for item in array {
                vector.push(try_from(item)?);
            }
            Ok(Value::Array(vector))
        }
        apache_avro::types::Value::Boolean(boolean) => Ok(Value::from(boolean)),
        apache_avro::types::Value::Bytes(bytes) => Ok(Value::from(bytes)),
        apache_avro::types::Value::Date(d) => Ok(Value::from(d)),
        apache_avro::types::Value::Decimal(ref d) => Ok(<Vec<u8>>::try_from(d)
            .map(|vec| Value::Array(vec.into_iter().map(Value::from).collect()))?),
        apache_avro::types::Value::Double(double) => Ok(Value::from(double)),
        apache_avro::types::Value::Duration(d) => Ok(Value::Array(
            <[u8; 12]>::from(d).into_iter().map(Value::from).collect(),
        )),
        apache_avro::types::Value::Enum(_, string) => Ok(Value::from(string)),
        apache_avro::types::Value::Fixed(_, bytes) => Ok(Value::from(bytes)),
        apache_avro::types::Value::Float(float) => Ok(Value::from(float as f64)),
        apache_avro::types::Value::Int(int) => Ok(Value::from(int)),
        apache_avro::types::Value::Long(long) => Ok(Value::from(long)),
        apache_avro::types::Value::Map(items) => items
            .into_iter()
            .map(|(key, value)| try_from(value).map(|v| (key, v)))
            .collect::<Result<Vec<_>, _>>()
            .map(|v| Value::Object(v.into_iter().collect())),
        apache_avro::types::Value::Null => Ok(Value::Null),
        apache_avro::types::Value::Record(items) => items
            .into_iter()
            .map(|(key, value)| try_from(value).map(|v| (key, v)))
            .collect::<Result<Vec<_>, _>>()
            .map(|v| Value::Object(v.into_iter().collect())),
        apache_avro::types::Value::String(string) => Ok(Value::from(string)),
        apache_avro::types::Value::TimeMicros(time_micros) => Ok(Value::from(time_micros)),
        apache_avro::types::Value::TimeMillis(time_millis) => Ok(Value::from(time_millis)),
        apache_avro::types::Value::TimestampMicros(timestamp_micros) => {
            Ok(Value::from(timestamp_micros))
        }
        apache_avro::types::Value::TimestampMillis(timestamp_millis) => {
            Ok(Value::from(timestamp_millis))
        }
        apache_avro::types::Value::Union(_, v) => try_from(*v),
        apache_avro::types::Value::Uuid(uuid) => Ok(Value::from(uuid.as_hyphenated().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use apache_avro::Schema;
    use bytes::BytesMut;

    use super::*;

    #[test]
    fn deserialize_avro() {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct Log {
            message: String,
        }

        let schema = r#"
            {
                "type": "record",
                "name": "log",
                "fields": [
                    {
                        "name": "message",
                        "type": "string"
                    }
                ]
            }
        "#
        .to_owned();
        let schema = Schema::parse_str(&schema).unwrap();

        let event = Log {
            message: "hello from avro".to_owned(),
        };
        let record_value = apache_avro::to_value(event).unwrap();
        let record_datum = apache_avro::to_avro_datum(&schema, record_value).unwrap();
        let record_bytes = Bytes::from(record_datum);

        let deserializer = AvroDeserializer::new(schema, false);
        let events = deserializer
            .parse(record_bytes, LogNamespace::Vector)
            .unwrap();
        assert_eq!(events.len(), 1);

        assert_eq!(
            events[0].as_log().get("message").unwrap(),
            &Value::from("hello from avro")
        );
    }

    #[test]
    fn deserialize_avro_strip_schema_id_prefix() {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct Log {
            message: String,
        }

        let schema = r#"
            {
                "type": "record",
                "name": "log",
                "fields": [
                    {
                        "name": "message",
                        "type": "string"
                    }
                ]
            }
        "#
        .to_owned();
        let schema = Schema::parse_str(&schema).unwrap();

        let event = Log {
            message: "hello from avro".to_owned(),
        };
        let record_value = apache_avro::to_value(event).unwrap();
        let record_datum = apache_avro::to_avro_datum(&schema, record_value).unwrap();

        let mut bytes = BytesMut::new();
        bytes.extend([0, 0, 0, 0, 0]); // 0 prefix + 4 byte schema id
        bytes.extend(record_datum);

        let deserializer = AvroDeserializer::new(schema, true);
        let events = deserializer
            .parse(bytes.freeze(), LogNamespace::Vector)
            .unwrap();
        assert_eq!(events.len(), 1);

        assert_eq!(
            events[0].as_log().get("message").unwrap(),
            &Value::from("hello from avro")
        );
    }
}
