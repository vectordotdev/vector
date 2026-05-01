use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_config::configurable_component;
use vector_core::{config::DataType, event::Event, schema};

use crate::encoding::BuildError;

/// Config used to build a `AvroSerializer`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AvroSerializerConfig {
    /// Options for the Avro serializer.
    pub avro: AvroSerializerOptions,
}

impl AvroSerializerConfig {
    /// Creates a new `AvroSerializerConfig`.
    pub const fn new(schema: String, schema_id: Option<i32>) -> Self {
        Self {
            avro: AvroSerializerOptions { schema, schema_id },
        }
    }

    /// Build the `AvroSerializer` from this configuration.
    pub fn build(&self) -> Result<AvroSerializer, BuildError> {
        let schema = apache_avro::Schema::parse_str(&self.avro.schema)
            .map_err(|error| format!("Failed building Avro serializer: {error}"))?;
        Ok(AvroSerializer {
            schema,
            schema_id: self.avro.schema_id,
        })
    }

    /// The data type of events that are accepted by `AvroSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        // TODO: Convert the Avro schema to a vector schema requirement.
        schema::Requirement::empty()
    }
}

/// Apache Avro serializer options.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct AvroSerializerOptions {
    /// The Avro schema.
    #[configurable(metadata(
        docs::examples = r#"{ "type": "record", "name": "log", "fields": [{ "name": "message", "type": "string" }] }"#
    ))]
    #[configurable(metadata(docs::human_name = "Schema JSON"))]
    pub schema: String,
    /// Confluent Avro schema ID
    ///
    /// When set, each message will use the [Confluent wire format][wire_format] (a 5-byte prefix
    /// containing a magic byte and a 4-byte big-endian schema ID).
    ///
    /// [wire_format]: https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format
    #[configurable(metadata(docs::examples = "42"))]
    pub schema_id: Option<i32>,
}

/// Serializer that converts an `Event` to bytes using the Apache Avro format.
#[derive(Debug, Clone)]
pub struct AvroSerializer {
    schema: apache_avro::Schema,
    schema_id: Option<i32>,
}

impl AvroSerializer {
    /// Creates a new `AvroSerializer`.
    pub const fn new(schema: apache_avro::Schema, schema_id: Option<i32>) -> Self {
        Self { schema, schema_id }
    }
}

impl Encoder<Event> for AvroSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let log = event.into_log();
        let value = apache_avro::to_value(log)?;
        let value = value.resolve(&self.schema)?;
        let bytes = apache_avro::to_avro_datum(&self.schema, value)?;

        if let Some(schema_id) = self.schema_id {
            buffer.put_slice(&[0x00]); // magic byte
            buffer.put_slice(&schema_id.to_be_bytes()); // schema id data
        }
        buffer.put_slice(&bytes);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use indoc::indoc;
    use vector_core::event::{LogEvent, Value};
    use vrl::btreemap;

    use super::*;

    #[test]
    fn serialize_avro() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "foo" => Value::from("bar")
        }));
        let schema = indoc! {r#"
            {
                "type": "record",
                "name": "Log",
                "fields": [
                    {
                        "name": "foo",
                        "type": ["string"]
                    }
                ]
            }
        "#}
        .to_owned();
        let config = AvroSerializerConfig::new(schema, None);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), b"\0\x06bar".as_slice());
    }

    #[test]
    fn serialize_avro_with_schema_id() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "foo" => Value::from("bar")
        }));
        let schema = indoc! {r#"
            {
                "type": "record",
                "name": "Log",
                "fields": [
                    {
                        "name": "foo",
                        "type": ["string"]
                    }
                ]
            }
        "#}
        .to_owned();
        let config = AvroSerializerConfig::new(schema, Some(42));
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), b"\0\0\0\0\x2A\0\x06bar".as_slice());
    }
}
