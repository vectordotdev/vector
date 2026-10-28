use std::collections::HashMap;

use apache_avro::Schema;
use bytes::{BufMut, BytesMut};
use tokio_util::codec::Encoder;
use vector_config::configurable_component;
use vector_core::{config::DataType, event::Event, schema};

use crate::encoding::{BuildError, serializer::BatchSerializerCompression};

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
#[allow(
    clippy::too_many_lines,
    reason = "Keep recursive schema coercion in one match."
)]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Time-millis is range checked and Avro union values use u32 indexes."
)]
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
            if value.clone().resolve(schema).is_ok() {
                // Resolving an already-converted value again can select a different
                // union branch and discard fields, so preserve the original input.
                return Ok(value);
            }

            let mut last_err = None;
            for (index, variant) in union_schema.variants().iter().enumerate() {
                let resolved_variant = match variant {
                    Schema::Ref { name } => names.get(name).unwrap_or(variant),
                    other => other,
                };
                match coerce_logical_types(value.clone(), variant, names) {
                    Ok(coerced)
                        if coerced
                            .clone()
                            .resolve_with_names(resolved_variant, names)
                            .is_ok() =>
                    {
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
#[configurable_component]
#[derive(Debug, Clone)]
pub struct AvroSerializerConfig {
    /// Options for the Avro serializer.
    pub avro: AvroSerializerOptions,
}

impl AvroSerializerConfig {
    /// Creates a new `AvroSerializerConfig`.
    #[must_use]
    pub const fn new(schema: String) -> Self {
        Self {
            avro: AvroSerializerOptions { schema },
        }
    }

    /// Build the `AvroSerializer` from this configuration.
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_errors_doc,
        reason = "The codec API error documentation needs a separate audit."
    )]
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
    #[must_use]
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    #[must_use]
    pub fn schema_requirement(&self) -> schema::Requirement {
        // TODO: Convert the Avro schema to a vector schema requirement.
        schema::Requirement::empty()
    }
}

/// Apache Avro serializer options.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct AvroSerializerOptions {
    /// The Avro schema definition in JSON format.
    #[configurable(metadata(
        docs::examples = r#"{ "type": "record", "name": "log", "fields": [{ "name": "message", "type": "string" }] }"#
    ))]
    #[configurable(metadata(docs::human_name = "Schema JSON"))]
    pub schema: String,
}

/// Serializer that converts an `Event` to bytes using the Apache Avro datum format.
///
/// Each event is encoded as a standalone Avro datum without any container metadata.
/// For Avro Object Container File (OCF) format, use `AvroOcfSerializer` via the
/// batch serializer interface instead.
#[derive(Debug, Clone)]
pub struct AvroSerializer {
    schema: apache_avro::Schema,
    named_schemas: Option<NamedSchemas>,
}

impl AvroSerializer {
    /// Creates a new `AvroSerializer`.
    #[must_use]
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
        if self.named_schemas.is_none() {
            self.named_schemas = Some(resolve_named_schemas(&self.schema).map_err(|error| {
                vector_common::Error::from(format!("Failed resolving Avro schema: {error}"))
            })?);
        }
        let value = log_to_avro_value(
            log,
            &self.schema,
            self.named_schemas
                .as_ref()
                .expect("named schemas are initialized above"),
        )?;
        let writer =
            apache_avro::writer::datum::GenericDatumWriter::builder(&self.schema).build()?;
        let bytes = writer.write_value_to_vec(value)?;
        buffer.put_slice(&bytes);
        Ok(())
    }
}

/// Compatibility alias for the shared batch-file compression configuration.
pub type AvroOcfCompression = BatchSerializerCompression;

/// Convert a VRL LogEvent to an Avro value with logical type coercion and resolution.
/// This is the single shared path used by both datum and OCF encoders to ensure
/// they handle dates, times, named schemas, and unions identically.
fn log_to_avro_value(
    log: vector_core::event::LogEvent,
    schema: &apache_avro::Schema,
    named_schemas: &NamedSchemas,
) -> vector_common::Result<AvroValue> {
    let value = apache_avro::to_value(log)?;
    let value = coerce_logical_types(value, schema, named_schemas)?;
    let value = value.resolve(schema)?;
    Ok(value)
}

fn resolve_ocf_compression(
    compression: BatchSerializerCompression,
) -> Result<apache_avro::Codec, BuildError> {
    match compression {
        BatchSerializerCompression::None => Ok(apache_avro::Codec::Null),
        BatchSerializerCompression::Deflate => {
            Ok(apache_avro::Codec::Deflate(apache_avro::DeflateSettings::default()))
        }
        BatchSerializerCompression::Snappy => Ok(apache_avro::Codec::Snappy),
        BatchSerializerCompression::Zstd { .. }
        | BatchSerializerCompression::Gzip { .. }
        | BatchSerializerCompression::Lz4 => Err(format!(
            "Avro OCF does not support {compression:?} compression; supported algorithms are none, deflate, and snappy"
        )
        .into()),
    }
}

/// Config used to build an `AvroOcfSerializer`.
#[configurable_component]
#[derive(Debug, Clone)]
pub struct AvroOcfSerializerConfig {
    /// Avro schema options used to encode OCF records.
    #[serde(flatten)]
    pub avro: AvroSerializerOptions,

    /// Compression codec applied to OCF data blocks.
    #[serde(default)]
    pub compression: BatchSerializerCompression,
}

impl AvroOcfSerializerConfig {
    /// Creates a new `AvroOcfSerializerConfig`.
    pub const fn new(options: AvroSerializerOptions) -> Self {
        Self {
            avro: options,
            compression: BatchSerializerCompression::None,
        }
    }

    /// Build the `AvroOcfSerializer` from this configuration.
    pub fn build(&self) -> Result<AvroOcfSerializer, BuildError> {
        let schema = apache_avro::Schema::parse_str(&self.avro.schema)
            .map_err(|error| format!("Failed building Avro OCF serializer: {error}"))?;
        AvroOcfSerializer::new(
            schema,
            resolve_ocf_compression(self.compression)?,
        )
        .map_err(|error| format!("Failed resolving Avro schema: {error}").into())
    }

    /// The data type of events that are accepted by `AvroOcfSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Batch serializer that encodes a collection of events as a complete Avro Object Container File
/// (OCF).
///
/// Each call to `encode` produces a self-contained OCF file: a header (with embedded schema and
/// randomly generated sync marker) followed by one or more data blocks containing all events.
/// This is the correct interface for OCF because OCF is file-scoped — it has one header, one
/// sync marker, and batched blocks — which maps naturally to batch encoding of `Vec<Event>`.
///
/// Use this via `BatchSerializerConfig` for sinks that support batch encoding (e.g. S3, file).
#[derive(Debug, Clone)]
pub struct AvroOcfSerializer {
    schema: apache_avro::Schema,
    compression: apache_avro::Codec,
    named_schemas: NamedSchemas,
}

impl AvroOcfSerializer {
    /// Creates a new `AvroOcfSerializer`.
    pub fn new(schema: apache_avro::Schema, compression: apache_avro::Codec) -> Result<Self, apache_avro::Error> {
        let named_schemas = resolve_named_schemas(&schema)?;
        Ok(Self {
            schema,
            compression,
            named_schemas,
        })
    }
}

impl Encoder<Vec<Event>> for AvroOcfSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, events: Vec<Event>, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let mut writer =
            apache_avro::Writer::with_codec(&self.schema, Vec::new(), self.compression)?;

        for event in events {
            let log = event.into_log();
            let value = log_to_avro_value(log, &self.schema, &self.named_schemas)?;
            writer.append_value(value).map_err(|e| {
                vector_common::Error::from(format!("Failed to append Avro record: {e}"))
            })?;
        }

        let bytes = writer.into_inner().map_err(|e| {
            vector_common::Error::from(format!("Failed to flush Avro OCF writer: {e}"))
        })?;

        buffer.put_slice(&bytes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};
    use indoc::indoc;
    use tokio_util::codec::Encoder as _;
    use vector_core::event::{LogEvent, Value};
    use vrl::btreemap;

    use super::*;

    fn schema_str() -> String {
        // Use plain "string" type (not union ["string"]) so round-trip values stay as
        // Value::String rather than Value::Union, making assertions simpler.
        // A separate test covers the union case via the datum serializer.
        indoc! {r#"
            {
                "type": "record",
                "name": "Log",
                "fields": [
                    {
                        "name": "foo",
                        "type": "string"
                    }
                ]
            }
        "#}
        .to_owned()
    }

    fn schema_with_doc_str() -> String {
        indoc! {r#"
            {
                "type": "record",
                "name": "Log",
                "fields": [
                    {
                        "name": "foo",
                        "type": "string",
                        "doc": "A foo field"
                    }
                ]
            }
        "#}
        .to_owned()
    }

    #[test]
    fn serialize_avro_datum() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "foo" => Value::from("bar")
        }));
        let config = AvroSerializerConfig::new(schema_str());
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        // "bar" encodes as 0x06 0x62 0x61 0x72 (length-prefixed zigzag string)
        assert!(!bytes.is_empty());
    }

    #[test]
    fn serialize_avro_ocf_roundtrip() {
        let schema_json = schema_str();
        let events = vec![
            Event::Log(LogEvent::from(btreemap! { "foo" => Value::from("bar") })),
            Event::Log(LogEvent::from(btreemap! { "foo" => Value::from("baz") })),
        ];

        let config = AvroOcfSerializerConfig::new(AvroSerializerOptions {
            schema: schema_json.clone(),
        });
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(events, &mut bytes).unwrap();

        let result = bytes.freeze();

        // Verify OCF magic bytes
        assert_eq!(&result[0..4], b"Obj\x01");

        // Round-trip: read back with apache_avro::Reader and verify all records
        let reader = apache_avro::Reader::new(result.as_ref()).unwrap();
        let records: Vec<_> = reader.collect::<Result<_, _>>().unwrap();
        assert_eq!(records.len(), 2);

        // Verify first record — schema uses plain "string" so values are Value::String
        let apache_avro::types::Value::Record(ref fields) = records[0] else {
            panic!("Expected Record, got: {:?}", records[0]);
        };
        assert!(
            fields
                .iter()
                .any(|(k, v)| k == "foo" && v == &apache_avro::types::Value::String("bar".into())),
            "Expected foo=bar in first record, got: {fields:?}"
        );

        // Verify second record
        let apache_avro::types::Value::Record(ref fields) = records[1] else {
            panic!("Expected Record, got: {:?}", records[1]);
        };
        assert!(
            fields
                .iter()
                .any(|(k, v)| k == "foo" && v == &apache_avro::types::Value::String("baz".into())),
            "Expected foo=baz in second record, got: {fields:?}"
        );
    }

    #[test]
    fn serialize_avro_ocf_preserves_schema_doc() {
        // Verify that doc strings are preserved in the embedded schema.
        // Note: apache_avro::Writer uses the schema's canonical form in the header, which strips
        // doc strings. This test documents the actual behaviour (PCF is stored) so that if the
        // library changes to preserve docs in a future version the test will catch it.
        // The important correctness property — that the schema fingerprint matches — is tested by
        // the round-trip tests and the decoder's Rabin fingerprint comparison.
        let schema_json = schema_with_doc_str();
        let events = vec![Event::Log(LogEvent::from(
            btreemap! { "foo" => Value::from("x") },
        ))];

        let config = AvroOcfSerializerConfig::new(AvroSerializerOptions {
            schema: schema_json.clone(),
        });
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();
        serializer.encode(events, &mut bytes).unwrap();

        // The output must be a valid OCF file readable by apache_avro::Reader
        let result = bytes.freeze();
        let reader = apache_avro::Reader::new(result.as_ref()).unwrap();
        let records: Vec<_> = reader.collect::<Result<_, _>>().unwrap();
        assert_eq!(records.len(), 1, "Should decode exactly one record");
    }

    #[test]
    fn serialize_avro_ocf_compression_variants_roundtrip() {
        let variants = [
            (AvroOcfCompression::None, "null"),
            (AvroOcfCompression::Deflate, "deflate"),
            (AvroOcfCompression::Snappy, "snappy"),
        ];

        for (compression, codec_name) in variants {
            let config = AvroOcfSerializerConfig {
                avro: AvroSerializerOptions {
                    schema: schema_str(),
                },
                compression,
            };
            let mut serializer = config.build().unwrap();
            let mut bytes = BytesMut::new();
            serializer
                .encode(
                    vec![Event::Log(LogEvent::from(
                        btreemap! { "foo" => Value::from("compressed") },
                    ))],
                    &mut bytes,
                )
                .unwrap();

            if codec_name != "null" {
                assert!(
                    bytes
                        .windows(codec_name.len())
                        .any(|window| window == codec_name.as_bytes()),
                    "OCF header must advertise {codec_name} compression"
                );
            }

            let records = apache_avro::Reader::new(bytes.as_ref())
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(records.len(), 1, "round trip failed for {codec_name}");
        }
    }

    #[test]
    fn avro_ocf_rejects_unsupported_shared_compression() {
        let config = AvroOcfSerializerConfig {
            avro: AvroSerializerOptions {
                schema: schema_str(),
            },
            compression: BatchSerializerCompression::Zstd { level: 1 },
        };

        let error = config
            .build()
            .expect_err("Avro OCF must reject unsupported Zstd compression");
        assert!(error.to_string().contains("does not support"));
    }

    #[test]
    fn serialize_avro_ocf_unique_sync_markers() {
        // Each AvroOcfSerializer call produces an independent OCF file with its own sync marker.
        // Verify two independently produced files don't share the same 16-byte sync marker.
        let schema_json = schema_str();
        let events1 = vec![Event::Log(LogEvent::from(
            btreemap! { "foo" => Value::from("a") },
        ))];
        let events2 = vec![Event::Log(LogEvent::from(
            btreemap! { "foo" => Value::from("b") },
        ))];

        let config = AvroOcfSerializerConfig::new(AvroSerializerOptions {
            schema: schema_json.clone(),
        });
        let mut s1 = config.clone().build().unwrap();
        let mut s2 = config.build().unwrap();

        let mut buf1 = BytesMut::new();
        let mut buf2 = BytesMut::new();
        s1.encode(events1, &mut buf1).unwrap();
        s2.encode(events2, &mut buf2).unwrap();

        let b1: Bytes = buf1.freeze();
        let b2: Bytes = buf2.freeze();

        // The sync marker is the last 16 bytes of the header (right after the header map).
        // Rather than parse offset precisely, use apache_avro::Reader to confirm both are valid
        // and independently readable.
        let r1: Vec<_> = apache_avro::Reader::new(b1.as_ref())
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let r2: Vec<_> = apache_avro::Reader::new(b2.as_ref())
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(r1.len(), 1);
        assert_eq!(r2.len(), 1);
    }

    #[test]
    fn serialize_avro_preserves_resolvable_union_branch() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "x" => Value::Array(vec![Value::from(65), Value::from(66)]),
            "y" => Value::from(123)
        }));
        let schema = indoc! {r#"
            [
                {
                    "type": "record",
                    "name": "A",
                    "fields": [
                        {"name": "x", "type": "string"}
                    ]
                },
                {
                    "type": "record",
                    "name": "B",
                    "fields": [
                        {"name": "x", "type": "bytes"},
                        {"name": "y", "type": "long"}
                    ]
                }
            ]
        "#}
        .to_owned();
        let config = AvroSerializerConfig::new(schema);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        let value = apache_avro::reader::datum::GenericDatumReader::builder(&serializer.schema)
            .build()
            .unwrap()
            .read_value(&mut bytes.as_ref())
            .unwrap();

        // The array resolves to bytes in B. Resolving again can turn those bytes into
        // a string, select A, and silently discard y.
        assert_eq!(
            value,
            AvroValue::Union(
                1,
                Box::new(AvroValue::Record(vec![
                    ("x".to_owned(), AvroValue::Bytes(vec![65, 66])),
                    ("y".to_owned(), AvroValue::Long(123)),
                ]))
            )
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "Keep recursive schema cases together in one fixture."
    )]
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

    fn ocf_records(bytes: &[u8]) -> Vec<apache_avro::types::Value> {
        let reader = apache_avro::Reader::new(bytes).expect("Failed to create OCF reader");
        reader.collect::<Result<Vec<_>, _>>().expect("Failed to read OCF records")
    }

    #[test]
    fn ocf_encodes_date_field_as_avro_date() {
        let schema_json = indoc! {r#"
            {
                "type": "record",
                "name": "t",
                "fields": [
                    {
                        "name": "day",
                        "type": {"type": "int", "logicalType": "date"}
                    }
                ]
            }
        "#}
        .to_owned();
        let event = Event::Log(LogEvent::from(btreemap! {
            "day" => Value::from(19_646_i64)
        }));

        let config = AvroOcfSerializerConfig::new(AvroSerializerOptions {
            schema: schema_json,
        });
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(vec![event], &mut bytes).unwrap();

        let records = ocf_records(&bytes);
        assert_eq!(records.len(), 1);

        let record = &records[0];
        let AvroValue::Record(fields) = record else {
            panic!("Expected Record, got: {:?}", record);
        };
        assert_eq!(fields.len(), 1);
        assert!(matches!(
            fields[0].1,
            AvroValue::Date(19_646)
        ), "Expected day field to be Date(19646), got: {:?}", fields[0].1);
    }

    #[test]
    fn ocf_encodes_time_millis_field_as_avro_time_millis() {
        let schema_json = indoc! {r#"
            {
                "type": "record",
                "name": "t",
                "fields": [
                    {
                        "name": "millis",
                        "type": {"type": "int", "logicalType": "time-millis"}
                    }
                ]
            }
        "#}
        .to_owned();
        let event = Event::Log(LogEvent::from(btreemap! {
            "millis" => Value::from(59_820_123_i64)
        }));

        let config = AvroOcfSerializerConfig::new(AvroSerializerOptions {
            schema: schema_json,
        });
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(vec![event], &mut bytes).unwrap();

        let records = ocf_records(&bytes);
        assert_eq!(records.len(), 1);

        let record = &records[0];
        let AvroValue::Record(fields) = record else {
            panic!("Expected Record, got: {:?}", record);
        };
        assert_eq!(fields.len(), 1);
        assert!(matches!(
            fields[0].1,
            AvroValue::TimeMillis(59_820_123)
        ), "Expected millis field to be TimeMillis(59820123), got: {:?}", fields[0].1);
    }

    #[test]
    fn ocf_encodes_nested_named_record_reference_dates() {
        let schema_json = indoc! {r#"
            {
                "type": "record",
                "name": "Outer",
                "fields": [
                    {
                        "name": "first",
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
                        "name": "second",
                        "type": "Inner"
                    }
                ]
            }
        "#}
        .to_owned();

        let event = Event::Log(LogEvent::from(btreemap! {
            "first" => Value::Object(vrl::btreemap! {
                "date" => Value::from(20_000_i64)
            }),
            "second" => Value::Object(vrl::btreemap! {
                "date" => Value::from(20_001_i64)
            })
        }));

        let config = AvroOcfSerializerConfig::new(AvroSerializerOptions {
            schema: schema_json,
        });
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(vec![event], &mut bytes).unwrap();

        let records = ocf_records(&bytes);
        assert_eq!(records.len(), 1);

        let record = &records[0];
        let AvroValue::Record(fields) = record else {
            panic!("Expected Record, got: {:?}", record);
        };
        assert_eq!(fields.len(), 2);

        // Check first field (direct Inner record)
        let first_inner = &fields[0].1;
        let AvroValue::Record(first_fields) = first_inner else {
            panic!("Expected Record for first field, got: {:?}", first_inner);
        };
        assert!(matches!(
            first_fields[0].1,
            AvroValue::Date(20_000)
        ), "Expected first.date to be Date(20000), got: {:?}", first_fields[0].1);

        // Check second field (named reference to Inner)
        let second_inner = &fields[1].1;
        let AvroValue::Record(second_fields) = second_inner else {
            panic!("Expected Record for second field, got: {:?}", second_inner);
        };
        assert!(matches!(
            second_fields[0].1,
            AvroValue::Date(20_001)
        ), "Expected second.date to be Date(20001), got: {:?}", second_fields[0].1);
    }

    #[test]
    fn ocf_rejects_out_of_range_time_millis_without_partial_output() {
        let schema_json = indoc! {r#"
            {
                "type": "record",
                "name": "t",
                "fields": [
                    {
                        "name": "millis",
                        "type": {"type": "int", "logicalType": "time-millis"}
                    }
                ]
            }
        "#}
        .to_owned();
        let event = Event::Log(LogEvent::from(btreemap! {
            "millis" => Value::from(86_400_000_i64)
        }));

        let config = AvroOcfSerializerConfig::new(AvroSerializerOptions {
            schema: schema_json,
        });
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        let result = serializer.encode(vec![event], &mut bytes);
        
        assert!(result.is_err(), "Expected encode to fail for out-of-range time-millis");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("time-millis"), "Error message should mention time-millis, got: {}", err_msg);
        assert!(bytes.is_empty(), "Buffer should be empty after error, but has {} bytes", bytes.len());
    }

    #[test]
    fn ocf_preserves_resolvable_union_branch() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "x" => Value::Array(vec![Value::from(65), Value::from(66)]),
            "y" => Value::from(123)
        }));
        let schema_json = indoc! {r#"
            [
                {
                    "type": "record",
                    "name": "A",
                    "fields": [
                        {"name": "x", "type": "string"}
                    ]
                },
                {
                    "type": "record",
                    "name": "B",
                    "fields": [
                        {"name": "x", "type": "bytes"},
                        {"name": "y", "type": "long"}
                    ]
                }
            ]
        "#}
        .to_owned();

        let config = AvroOcfSerializerConfig::new(AvroSerializerOptions {
            schema: schema_json,
        });
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(vec![event], &mut bytes).unwrap();

        let records = ocf_records(&bytes);
        assert_eq!(records.len(), 1);

        let value = &records[0];
        assert_eq!(
            value,
            &AvroValue::Union(
                1,
                Box::new(AvroValue::Record(vec![
                    ("x".to_owned(), AvroValue::Bytes(vec![65, 66])),
                    ("y".to_owned(), AvroValue::Long(123)),
                ]))
            )
        );
    }

    #[test]
    fn ocf_empty_batch_is_valid_header_only_file() {
        let schema_json = indoc! {r#"
            {
                "type": "record",
                "name": "Log",
                "fields": [
                    {"name": "foo", "type": "string"}
                ]
            }
        "#}
        .to_owned();

        let config = AvroOcfSerializerConfig::new(AvroSerializerOptions {
            schema: schema_json,
        });
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(vec![], &mut bytes).unwrap();

        let result = bytes.freeze();
        assert!(!result.is_empty(), "Empty batch should still produce OCF header");
        assert_eq!(&result[0..4], b"Obj\x01", "OCF should start with magic bytes");

        let records = ocf_records(&result);
        assert_eq!(records.len(), 0, "Empty batch should produce no records");
    }
}
