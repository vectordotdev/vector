use std::collections::HashMap;

use apache_avro::Schema;
use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_config::configurable_component;
use vector_core::{config::DataType, event::Event, schema};

use crate::encoding::BuildError;

type AvroValue = apache_avro::types::Value;
type NamedSchemas = HashMap<apache_avro::schema::Name, apache_avro::Schema>;

fn resolve_named_schemas(schema: &apache_avro::Schema) -> Result<NamedSchemas, apache_avro::Error> {
    let resolved = apache_avro::schema::ResolvedSchema::try_from(schema)?;
    Ok(resolved
        .get_names()
        .iter()
        .map(|(name, schema)| (name.clone(), (*schema).clone()))
        .collect())
}

/// `apache_avro::to_value` serializes VRL values into Avro types which may not
/// resolve against certain logical type schemas
/// (e.g. VRL integer (i64) -> Avro `Long` which cannot resolve to Avro `Date`).
/// This does a recursive pre-pass to coerce such values before resolution.
fn coerce_logical_types(
    value: AvroValue,
    schema: &apache_avro::Schema,
    names: &NamedSchemas,
) -> vector_common::Result<AvroValue> {
    match (value, schema) {
        (AvroValue::Long(days), Schema::Date) => {
            i32::try_from(days).map(AvroValue::Date).map_err(|_| {
                vector_common::Error::from(format!(
                    "Avro date value {days} is out of range for i32"
                ))
            })
        }
        (AvroValue::Long(millis), Schema::TimeMillis) => {
            const MILLIS_PER_DAY: i64 = 86_400_000;
            if !(0..MILLIS_PER_DAY).contains(&millis) {
                return Err(vector_common::Error::from(format!(
                    "Avro time-millis value {millis} is out of range (must be 0..={MILLIS_PER_DAY} - 1)"
                )));
            }
            Ok(AvroValue::TimeMillis(millis as i32))
        }
        (AvroValue::Record(fields), Schema::Record(record_schema)) => {
            let fields = fields
                .into_iter()
                .map(|(name, value)| {
                    let value = match record_schema.lookup.get(&name) {
                        Some(index) => {
                            let field_schema = &record_schema.fields[*index].schema;
                            coerce_logical_types(value, field_schema, names)?
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
                            coerce_logical_types(value, field_schema, names)?
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
            .map(|item| coerce_logical_types(item, &array_schema.items, names))
            .collect::<Result<Vec<_>, _>>()
            .map(AvroValue::Array),
        (AvroValue::Map(entries), Schema::Map(map_schema)) => entries
            .into_iter()
            .map(|(key, value)| {
                coerce_logical_types(value, &map_schema.types, names).map(|value| (key, value))
            })
            .collect::<vector_common::Result<_>>()
            .map(AvroValue::Map),
        (AvroValue::Union(index, value), Schema::Union(union_schema)) => {
            let schema = union_schema
                .variants()
                .get(index as usize)
                .unwrap_or(schema);
            coerce_logical_types(*value, schema, names)
                .map(|value| AvroValue::Union(index, Box::new(value)))
        }
        (value, Schema::Union(union_schema)) => {
            if let Ok(resolved) = value.clone().resolve(schema) {
                return Ok(resolved);
            }

            let mut last_err = None;
            for (index, variant) in union_schema.variants().iter().enumerate() {
                let resolved_variant = match variant {
                    Schema::Ref { name } => names.get(name).unwrap_or(variant),
                    other => other,
                };
                match coerce_logical_types(value.clone(), variant, names) {
                    Ok(coerced) if coerced.clone().resolve(resolved_variant).is_ok() => {
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
        (value, Schema::Ref { name }) => {
            let schema = names.get(name).ok_or_else(|| {
                vector_common::Error::from(format!("Unknown schema ref: {}", name.fullname(None)))
            })?;
            coerce_logical_types(value, schema, names)
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
        let named_schemas = resolve_named_schemas(&schema)
            .map_err(|error| format!("Failed resolving Avro schema: {error}"))?;
        Ok(AvroSerializer {
            schema,
            named_schemas: Some(named_schemas),
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
}

/// Serializer that converts an `Event` to bytes using the Apache Avro format.
#[derive(Debug, Clone)]
pub struct AvroSerializer {
    schema: apache_avro::Schema,
    named_schemas: Option<NamedSchemas>,
}

impl AvroSerializer {
    /// Creates a new `AvroSerializer`.
    pub const fn new(schema: apache_avro::Schema) -> Self {
        Self {
            schema,
            named_schemas: None,
        }
    }
}

impl Encoder<Event> for AvroSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let log = event.into_log();
        let value = apache_avro::to_value(log)?;
        if self.named_schemas.is_none() {
            self.named_schemas = Some(resolve_named_schemas(&self.schema).map_err(|error| {
                vector_common::Error::from(format!("Failed resolving Avro schema: {error}"))
            })?);
        }
        let value = coerce_logical_types(
            value,
            &self.schema,
            self.named_schemas
                .as_ref()
                .expect("named schemas are initialized above"),
        )?;
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
                        "name": "inner_reference",
                        "type": "Inner"
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
                "inner_reference".to_owned(),
                AvroValue::Record(vec![("date".to_owned(), AvroValue::Long(20_003))]),
            ),
            (
                "date_array".to_owned(),
                AvroValue::Array(vec![AvroValue::Long(20_004), AvroValue::Long(20_005)]),
            ),
            (
                "date_map".to_owned(),
                AvroValue::Map(
                    [
                        ("first".to_owned(), AvroValue::Long(20_006)),
                        ("second".to_owned(), AvroValue::Long(20_007)),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
            ("union_date".to_owned(), AvroValue::Long(20_008)),
            ("fallback_union_date".to_owned(), AvroValue::Long(20_010)),
            (
                "logical_only_union_date".to_owned(),
                AvroValue::Long(20_009),
            ),
        ]);

        let named_schemas = resolve_named_schemas(&schema).unwrap();
        let value = coerce_logical_types(value, &schema, &named_schemas).unwrap();
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
                        AvroValue::Record(inner) if matches!(inner[0].1, AvroValue::Date(20_003))
                    )
                    && matches!(
                        &fields[4].1,
                        AvroValue::Array(items)
                            if matches!(items.as_slice(), [AvroValue::Date(20_004), AvroValue::Date(20_005)])
                    )
                    && matches!(
                        &fields[5].1,
                        AvroValue::Map(entries)
                            if matches!(entries.get("first"), Some(AvroValue::Date(20_006)))
                                && matches!(entries.get("second"), Some(AvroValue::Date(20_007)))
                    )
                    && matches!(
                        &fields[6].1,
                        AvroValue::Union(1, value) if matches!(value.as_ref(), AvroValue::Date(20_008))
                    )
                    && matches!(
                        &fields[7].1,
                        AvroValue::Union(2, value)
                            if matches!(value.as_ref(), AvroValue::Long(20_010))
                    )
                    && matches!(
                        &fields[8].1,
                        AvroValue::Union(1, value) if matches!(value.as_ref(), AvroValue::Date(20_009))
                    )
            }
        ));
    }

    #[test]
    fn coerce_nullable_named_record_with_logical_types() {
        // Regression: a union branch that is a named schema reference (e.g. ["null", "Inner"])
        // must resolve coerced values against the dereferenced schema, not the bare Schema::Ref.
        let schema = apache_avro::Schema::parse_str(indoc! {r#"
            {
                "type": "record",
                "name": "Outer",
                "fields": [
                    {
                        "name": "inner",
                        "type": {
                            "type": "record",
                            "name": "Inner",
                            "fields": [
                                {
                                    "name": "date",
                                    "type": {"type": "int", "logicalType": "date"}
                                },
                                {
                                    "name": "time_millis",
                                    "type": {"type": "int", "logicalType": "time-millis"}
                                }
                            ]
                        }
                    },
                    {
                        "name": "nullable_inner",
                        "type": ["null", "Inner"]
                    }
                ]
            }
        "#})
        .unwrap();

        let value = AvroValue::Record(vec![
            (
                "inner".to_owned(),
                AvroValue::Record(vec![
                    ("date".to_owned(), AvroValue::Long(20_000)),
                    ("time_millis".to_owned(), AvroValue::Long(43_200_000)),
                ]),
            ),
            (
                "nullable_inner".to_owned(),
                AvroValue::Record(vec![
                    ("date".to_owned(), AvroValue::Long(20_001)),
                    ("time_millis".to_owned(), AvroValue::Long(3_600_000)),
                ]),
            ),
        ]);

        let named_schemas = resolve_named_schemas(&schema).unwrap();
        let value = coerce_logical_types(value, &schema, &named_schemas).unwrap();
        let value = value.resolve(&schema).unwrap();

        assert!(matches!(
            value,
            AvroValue::Record(ref fields) if {
                matches!(
                    &fields[0].1,
                    AvroValue::Record(inner) if {
                        matches!(inner[0].1, AvroValue::Date(20_000))
                            && matches!(inner[1].1, AvroValue::TimeMillis(43_200_000))
                    }
                )
                && matches!(
                    &fields[1].1,
                    AvroValue::Union(1, value) if matches!(
                        value.as_ref(),
                        AvroValue::Record(inner) if {
                            matches!(inner[0].1, AvroValue::Date(20_001))
                                && matches!(inner[1].1, AvroValue::TimeMillis(3_600_000))
                        }
                    )
                )
            }
        ));
    }

    #[test]
    fn rejects_time_millis_out_of_range() {
        let schema = apache_avro::Schema::parse_str(indoc! {r#"
            {
                "type": "record",
                "name": "Log",
                "fields": [
                    {
                        "name": "time",
                        "type": {"type": "int", "logicalType": "time-millis"}
                    }
                ]
            }
        "#})
        .unwrap();
        let named_schemas = resolve_named_schemas(&schema).unwrap();

        // (Valid) minimum value
        let value = AvroValue::Record(vec![("time".to_owned(), AvroValue::Long(0))]);
        let coerced = coerce_logical_types(value, &schema, &named_schemas).unwrap();
        assert!(matches!(
            coerced,
            AvroValue::Record(ref fields) if matches!(fields[0].1, AvroValue::TimeMillis(0))
        ));

        // (Valid): maximum value (MILLIS_PER_DAY - 1)
        let value = AvroValue::Record(vec![("time".to_owned(), AvroValue::Long(86_399_999))]);
        let coerced = coerce_logical_types(value, &schema, &named_schemas).unwrap();
        assert!(matches!(
            coerced,
            AvroValue::Record(ref fields) if matches!(fields[0].1, AvroValue::TimeMillis(86_399_999))
        ));

        // (Invalid) negative value
        let value = AvroValue::Record(vec![("time".to_owned(), AvroValue::Long(-1))]);
        assert!(coerce_logical_types(value, &schema, &named_schemas).is_err());

        // (Invalid) equal to MILLIS_PER_DAY
        let value = AvroValue::Record(vec![("time".to_owned(), AvroValue::Long(86_400_000))]);
        assert!(coerce_logical_types(value, &schema, &named_schemas).is_err());

        // (Invalid) greater than MILLIS_PER_DAY
        let value = AvroValue::Record(vec![("time".to_owned(), AvroValue::Long(90_000_000))]);
        assert!(coerce_logical_types(value, &schema, &named_schemas).is_err());
    }
}
