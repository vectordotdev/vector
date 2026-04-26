use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_config::configurable_component;
use vector_core::{config::DataType, event::Event, schema};

use crate::encoding::BuildError;

type AvroValue = apache_avro::types::Value;

/// `apache_avro::to_value` may serialize VRL values into Avro types which later
/// cannot be resolved against certain Avro types
/// (e.g. VRL integer (i64) -> Avro `Long` which cannot be resolved to Avro `Date`).
/// `coerce_logical_types` does a recursive pre-pass to fix such cases.
fn coerce_logical_types(
    value: AvroValue,
    schema: &apache_avro::Schema,
) -> vector_common::Result<AvroValue> {
    use apache_avro::Schema;
    match (value, schema) {
        (AvroValue::Long(days), Schema::Date) => {
            i32::try_from(days).map(AvroValue::Date).map_err(|_| {
                vector_common::Error::from(format!(
                    "Avro date value {days} is out of range for i32"
                ))
            })
        }
        (AvroValue::Long(millis), Schema::TimeMillis) => i32::try_from(millis)
            .map(AvroValue::TimeMillis)
            .map_err(|_| {
                vector_common::Error::from(format!(
                    "Avro time-millis value {millis} is out of range for i32"
                ))
            }),
        (AvroValue::Record(fields), Schema::Record(record_schema)) => {
            let fields = fields
                .into_iter()
                .map(|(name, value)| {
                    let value = match record_schema.lookup.get(&name) {
                        Some(index) => {
                            let field_schema = &record_schema.fields[*index].schema;
                            coerce_logical_types(value, field_schema)?
                        }
                        None => value,
                    };
                    Ok((name, value))
                })
                .collect::<vector_common::Result<Vec<_>>>()?;
            Ok(AvroValue::Record(fields))
        }
        (AvroValue::Map(entries), Schema::Record(record_schema)) => {
            let entries = entries
                .into_iter()
                .map(|(name, value)| {
                    let value = match record_schema.lookup.get(&name) {
                        Some(index) => {
                            let field_schema = &record_schema.fields[*index].schema;
                            coerce_logical_types(value, field_schema)?
                        }
                        None => value,
                    };
                    Ok((name, value))
                })
                .collect::<vector_common::Result<_>>()?;
            Ok(AvroValue::Map(entries))
        }
        (AvroValue::Array(items), Schema::Array(array_schema)) => items
            .into_iter()
            .map(|item| coerce_logical_types(item, &array_schema.items))
            .collect::<Result<Vec<_>, _>>()
            .map(AvroValue::Array),
        (AvroValue::Map(entries), Schema::Map(map_schema)) => entries
            .into_iter()
            .map(|(key, value)| {
                coerce_logical_types(value, &map_schema.types).map(|value| (key, value))
            })
            .collect::<vector_common::Result<_>>()
            .map(AvroValue::Map),
        (AvroValue::Union(index, value), Schema::Union(union_schema)) => {
            let schema = union_schema
                .variants()
                .get(index as usize)
                .unwrap_or(schema);
            coerce_logical_types(*value, schema)
                .map(|value| AvroValue::Union(index, Box::new(value)))
        }
        (value, Schema::Union(union_schema)) => {
            if let Ok(resolved) = value.clone().resolve(schema) {
                return Ok(resolved);
            }

            let mut last_err = None;
            for (index, variant) in union_schema.variants().iter().enumerate() {
                match coerce_logical_types(value.clone(), variant) {
                    Ok(coerced) if coerced.clone().resolve(variant).is_ok() => {
                        return Ok(AvroValue::Union(index as u32, Box::new(coerced)));
                    }
                    Ok(_) => {}
                    Err(err) => last_err = Some(err),
                }
            }

            match last_err {
                Some(err) => Err(err),
                None => Ok(value),
            }
        }
        (value, _) => Ok(value),
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
        let value = apache_avro::to_value(log)?;
        let value = coerce_logical_types(value, &self.schema)?;
        let value = value.resolve(&self.schema)?;
        let bytes = apache_avro::to_avro_datum(&self.schema, value)?;
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

    #[test]
    fn coerce_date_fields_recursively() {
        let schema = apache_avro::Schema::parse_str(indoc! {r#"
            {
                "type": "record",
                "name": "Outer",
                "fields": [
                    {
                        "name": "direct_date",
                        "type": {"type": "int", "logicalType": "date"}
                    },
                    {
                        "name": "inner",
                        "type": {
                            "type": "record",
                            "name": "Inner",
                            "fields": [
                                {
                                    "name": "date",
                                    "type": {"type": "int", "logicalType": "date"}
                                }
                            ]
                        }
                    },
                    {
                        "name": "record_as_map",
                        "type": {
                            "type": "record",
                            "name": "MapBackedInner",
                            "fields": [
                                {
                                    "name": "date",
                                    "type": {"type": "int", "logicalType": "date"}
                                }
                            ]
                        }
                    },
                    {
                        "name": "date_array",
                        "type": {
                            "type": "array",
                            "items": {"type": "int", "logicalType": "date"}
                        }
                    },
                    {
                        "name": "date_map",
                        "type": {
                            "type": "map",
                            "values": {"type": "int", "logicalType": "date"}
                        }
                    },
                    {
                        "name": "union_date",
                        "type": ["null", {"type": "int", "logicalType": "date"}]
                    },
                    {
                        "name": "fallback_union_date",
                        "type": [
                            "null",
                            {"type": "int", "logicalType": "date"},
                            "long"
                        ]
                    },
                    {
                        "name": "logical_only_union_date",
                        "type": [
                            "null",
                            {"type": "int", "logicalType": "date"}
                        ]
                    }
                ]
            }
        "#})
        .unwrap();
        let value = AvroValue::Record(vec![
            ("direct_date".to_owned(), AvroValue::Long(20_000)),
            (
                "inner".to_owned(),
                AvroValue::Record(vec![("date".to_owned(), AvroValue::Long(20_001))]),
            ),
            (
                "record_as_map".to_owned(),
                AvroValue::Map(
                    [("date".to_owned(), AvroValue::Long(20_002))]
                        .into_iter()
                        .collect(),
                ),
            ),
            (
                "date_array".to_owned(),
                AvroValue::Array(vec![AvroValue::Long(20_003), AvroValue::Long(20_004)]),
            ),
            (
                "date_map".to_owned(),
                AvroValue::Map(
                    [
                        ("first".to_owned(), AvroValue::Long(20_005)),
                        ("second".to_owned(), AvroValue::Long(20_006)),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
            ("union_date".to_owned(), AvroValue::Long(20_007)),
            ("fallback_union_date".to_owned(), AvroValue::Long(20_009)),
            (
                "logical_only_union_date".to_owned(),
                AvroValue::Long(20_008),
            ),
        ]);

        let value = coerce_logical_types(value, &schema).unwrap();
        let value = value.resolve(&schema).unwrap();

        assert!(matches!(
            value,
            AvroValue::Record(fields) if {
                matches!(fields[0].1, AvroValue::Date(20_000))
                    && matches!(
                        &fields[1].1,
                        AvroValue::Record(inner) if matches!(inner[0].1, AvroValue::Date(20_001))
                    )
                    && matches!(
                        &fields[2].1,
                        AvroValue::Record(inner) if matches!(inner[0].1, AvroValue::Date(20_002))
                    )
                    && matches!(
                        &fields[3].1,
                        AvroValue::Array(items)
                            if matches!(items.as_slice(), [AvroValue::Date(20_003), AvroValue::Date(20_004)])
                    )
                    && matches!(
                        &fields[4].1,
                        AvroValue::Map(entries)
                            if matches!(entries.get("first"), Some(AvroValue::Date(20_005)))
                                && matches!(entries.get("second"), Some(AvroValue::Date(20_006)))
                    )
                    && matches!(
                        &fields[5].1,
                        AvroValue::Union(1, value) if matches!(value.as_ref(), AvroValue::Date(20_007))
                    )
                    && matches!(
                        &fields[6].1,
                        AvroValue::Union(2, value)
                            if matches!(value.as_ref(), AvroValue::Long(20_009))
                    )
                    && matches!(
                        &fields[7].1,
                        AvroValue::Union(1, value) if matches!(value.as_ref(), AvroValue::Date(20_008))
                    )
            }
        ));
    }
}
