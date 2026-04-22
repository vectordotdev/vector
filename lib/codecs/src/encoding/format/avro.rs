use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use uuid::Uuid;
use vector_config::configurable_component;
use vector_core::{config::DataType, event::Event, schema};

use crate::encoding::BuildError;

type VrlValue = vrl::value::Value;
type AvroValue = apache_avro::types::Value;

/// Converts a VRL [`Value`](VrlValue) to an [`apache_avro::types::Value`] using the provided
/// schema to resolve ambiguous types (e.g. `Integer` -> `Int` vs `Date`).
pub(crate) fn to_avro(
    value: &VrlValue,
    schema: &apache_avro::Schema,
    names: &apache_avro::schema::NamesRef<'_>,
) -> vector_common::Result<AvroValue> {
    use apache_avro::Schema;
    match (value, schema) {
        (VrlValue::Null, Schema::Null) => Ok(AvroValue::Null),

        (VrlValue::Boolean(b), Schema::Boolean) => Ok(AvroValue::Boolean(*b)),

        (VrlValue::Integer(i), Schema::Int) => i32::try_from(*i)
            .map(AvroValue::Int)
            .map_err(|_| vector_common::Error::from(format!("Integer {i} overflows Avro int (i32)"))),
        (VrlValue::Integer(i), Schema::Date) => i32::try_from(*i)
            .map(AvroValue::Date)
            .map_err(|_| vector_common::Error::from(format!("Integer {i} overflows Avro date (i32)"))),
        (VrlValue::Integer(i), Schema::TimeMillis) => i32::try_from(*i)
            .map(AvroValue::TimeMillis)
            .map_err(|_| {
                vector_common::Error::from(format!("Integer {i} overflows Avro time-millis (i32)"))
            }),

        (VrlValue::Integer(i), Schema::Long) => Ok(AvroValue::Long(*i)),
        (VrlValue::Integer(i), Schema::TimeMicros) => Ok(AvroValue::TimeMicros(*i)),
        (VrlValue::Integer(i), Schema::TimestampMillis) => Ok(AvroValue::TimestampMillis(*i)),
        (VrlValue::Integer(i), Schema::TimestampMicros) => Ok(AvroValue::TimestampMicros(*i)),
        (VrlValue::Integer(i), Schema::LocalTimestampMillis) => {
            Ok(AvroValue::LocalTimestampMillis(*i))
        }
        (VrlValue::Integer(i), Schema::LocalTimestampMicros) => {
            Ok(AvroValue::LocalTimestampMicros(*i))
        }

        (VrlValue::Float(f), Schema::Float) => Ok(AvroValue::Float(f.into_inner() as f32)),
        (VrlValue::Float(f), Schema::Double) => Ok(AvroValue::Double(f.into_inner())),

        (VrlValue::Bytes(b), Schema::Bytes) => Ok(AvroValue::Bytes(b.to_vec())),
        (VrlValue::Bytes(b), Schema::String) => String::from_utf8(b.to_vec())
            .map(AvroValue::String)
            .map_err(|e| {
                vector_common::Error::from(format!("Invalid UTF-8 in string field: {e}"))
            }),
        (VrlValue::Regex(b), Schema::String) => Ok(AvroValue::String(b.as_str().to_owned())),

        (VrlValue::Bytes(b), Schema::Uuid) => {
            let s = String::from_utf8(b.to_vec()).map_err(|e| {
                vector_common::Error::from(format!("Invalid UTF-8 in UUID field: {e}"))
            })?;
            Uuid::parse_str(&s)
                .map(AvroValue::Uuid)
                .map_err(|e| vector_common::Error::from(format!("Invalid UUID: {e}")))
        }

        (VrlValue::Bytes(b), Schema::Enum(enum_schema)) => {
            let s = String::from_utf8(b.to_vec()).map_err(|e| {
                vector_common::Error::from(format!("Invalid UTF-8 in enum field: {e}"))
            })?;
            let index = enum_schema
                .symbols
                .iter()
                .position(|sym| sym == &s)
                .ok_or_else(|| vector_common::Error::from(format!("Unknown enum symbol: {s}")))?;
            Ok(AvroValue::Enum(index as u32, s))
        }

        (VrlValue::Timestamp(ts), Schema::TimestampMillis) => {
            Ok(AvroValue::TimestampMillis(ts.timestamp_millis()))
        }
        (VrlValue::Timestamp(ts), Schema::TimestampMicros) => {
            Ok(AvroValue::TimestampMicros(ts.timestamp_micros()))
        }
        (VrlValue::Timestamp(ts), Schema::LocalTimestampMillis) => {
            Ok(AvroValue::LocalTimestampMillis(ts.timestamp_millis()))
        }
        (VrlValue::Timestamp(ts), Schema::LocalTimestampMicros) => {
            Ok(AvroValue::LocalTimestampMicros(ts.timestamp_micros()))
        }
        (VrlValue::Timestamp(ts), Schema::Long) => Ok(AvroValue::Long(ts.timestamp_millis())),
        (VrlValue::Timestamp(ts), Schema::String) => Ok(AvroValue::String(
            ts.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
        )),

        (v, Schema::Ref { name }) => {
            let resolved = names.get(name).ok_or_else(|| {
                vector_common::Error::from(format!("Unknown schema ref: {}", name.fullname(None)))
            })?;
            to_avro(v, resolved, names)
        }

        (VrlValue::Array(items), Schema::Array(array_schema)) => items
            .iter()
            .map(|item| to_avro(item, &array_schema.items, names))
            .collect::<Result<Vec<_>, _>>()
            .map(AvroValue::Array),

        (VrlValue::Object(map), Schema::Map(map_schema)) => map
            .iter()
            .map(|(k, v)| to_avro(v, &map_schema.types, names).map(|av| (k.to_string(), av)))
            .collect::<Result<Vec<_>, _>>()
            .map(|items| AvroValue::Map(items.into_iter().collect())),

        (VrlValue::Object(map), Schema::Record(record_schema)) => {
            let fields = record_schema
                .fields
                .iter()
                .map(|field| {
                    let av = match map.get(field.name.as_str()) {
                        Some(v) => to_avro(v, &field.schema, names)?,
                        None => match &field.default {
                            Some(json_default) => {
                                AvroValue::from(json_default.clone()).resolve(&field.schema)?
                            }
                            None => {
                                return Err(vector_common::Error::from(format!(
                                    "Missing record field: {}",
                                    field.name
                                )))
                            }
                        },
                    };
                    Ok((field.name.clone(), av))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(AvroValue::Record(fields))
        }

        (v, Schema::Union(union_schema)) => {
            // Prefer null variant for Null values, otherwise try each variant in order
            if matches!(v, VrlValue::Null)
                && let Some(idx) = union_schema
                    .variants()
                    .iter()
                    .position(|s| matches!(s, Schema::Null))
            {
                return Ok(AvroValue::Union(idx as u32, Box::new(AvroValue::Null)));
            }
            for (idx, variant_schema) in union_schema.variants().iter().enumerate() {
                if matches!(variant_schema, Schema::Null) {
                    continue;
                }
                if let Ok(av) = to_avro(v, variant_schema, names) {
                    return Ok(AvroValue::Union(idx as u32, Box::new(av)));
                }
            }
            Err(vector_common::Error::from(format!(
                "No matching union variant for value of kind {}",
                v.kind_str()
            )))
        }

        (v, s) => Err(vector_common::Error::from(format!(
            "Cannot convert VRL {} to Avro schema {:?}",
            v.kind_str(),
            s
        ))),
    }
}

/// Config used to build a `AvroSerializer`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AvroSerializerConfig {
    /// Options for the Avro serializer.
    pub avro: AvroSerializerOptions,
}

impl AvroSerializerConfig {
    /// Creates a new `AvroSerializerConfig`.
    pub const fn new(schema: String) -> Self {
        Self {
            avro: AvroSerializerOptions { schema },
        }
    }

    /// Build the `AvroSerializer` from this configuration.
    pub fn build(&self) -> Result<AvroSerializer, BuildError> {
        let schema = apache_avro::Schema::parse_str(&self.avro.schema)
            .map_err(|error| format!("Failed building Avro serializer: {error}"))?;
        Ok(AvroSerializer { schema })
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
}

/// Serializer that converts an `Event` to bytes using the Apache Avro format.
#[derive(Debug, Clone)]
pub struct AvroSerializer {
    schema: apache_avro::Schema,
}

impl AvroSerializer {
    /// Creates a new `AvroSerializer`.
    pub const fn new(schema: apache_avro::Schema) -> Self {
        Self { schema }
    }
}

impl Encoder<Event> for AvroSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let log = event.into_log();
        let (value, _metadata) = log.into_parts();
        let resolved = apache_avro::schema::ResolvedSchema::try_from(&self.schema)
            .map_err(|e| vector_common::Error::from(format!("Failed resolving Avro schema: {e}")))?;
        let names = resolved.get_names();
        let avro_value = to_avro(&value, &self.schema, names)?;
        let bytes = apache_avro::to_avro_datum(&self.schema, avro_value)?;
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
        let config = AvroSerializerConfig::new(schema);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), b"\0\x06bar".as_slice());
    }
}
