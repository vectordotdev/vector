//! Parquet batch format codec for batched event encoding
//!
//! Provides Apache Parquet format encoding with static schema support.
//! This reuses the Arrow record batch building logic from the Arrow IPC codec,
//! then writes the batch as a complete Parquet file using `ArrowWriter`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef, TimeUnit};
use bytes::{BufMut, BytesMut};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression as ParquetCodecCompression;
use parquet::file::properties::WriterProperties;
use prost_reflect::Kind;
use tracing::warn;
use vector_config::configurable_component;
use vector_core::event::Event;

use super::arrow::{ArrowEncodingError, build_record_batch};

/// Parquet compression codec options.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParquetCompression {
    /// No compression.
    None,
    /// Snappy compression.
    #[default]
    Snappy,
    /// Zstandard compression.
    Zstd,
    /// Gzip compression.
    Gzip,
    /// LZ4 raw compression.
    Lz4,
}

impl ParquetCompression {
    fn to_parquet_compression(self) -> ParquetCodecCompression {
        match self {
            Self::None => ParquetCodecCompression::UNCOMPRESSED,
            Self::Snappy => ParquetCodecCompression::SNAPPY,
            Self::Zstd => ParquetCodecCompression::ZSTD(Default::default()),
            Self::Gzip => ParquetCodecCompression::GZIP(Default::default()),
            Self::Lz4 => ParquetCodecCompression::LZ4_RAW,
        }
    }
}

/// Schema handling mode.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchemaMode {
    /// Missing fields become null. Extra fields are silently dropped.
    #[default]
    Relaxed,
    /// Missing fields become null. Extra fields cause an error.
    Strict,
}

/// Arrow data type for Parquet schema field definitions.
///
/// Scalar types map directly to Arrow data types. Compound types (`struct`,
/// `list`, `map`) support one level of nesting via the `fields`, `items`,
/// `key_type`, and `value_type` properties on the field definition.
/// For deeper nesting, use `parquet_schema`, `avro_schema`, or `proto_desc_file`.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParquetFieldType {
    /// Boolean values.
    Boolean,
    /// 32-bit signed integer.
    Int32,
    /// 64-bit signed integer.
    Int64,
    /// 32-bit floating point.
    Float32,
    /// 64-bit floating point.
    Float64,
    /// UTF-8 string.
    Utf8,
    /// Binary data.
    Binary,
    /// Timestamp with millisecond precision (UTC).
    TimestampMillisecond,
    /// Timestamp with microsecond precision (UTC).
    TimestampMicrosecond,
    /// Timestamp with nanosecond precision (UTC).
    TimestampNanosecond,
    /// Date (days since epoch).
    Date32,
    /// Struct (nested record). Define sub-fields via the `fields` property.
    Struct,
    /// List (repeated values). Define element type via the `items` property.
    List,
    /// Map (key-value pairs). Define types via `key_type` and `value_type`.
    Map,
}

impl ParquetFieldType {
    /// Returns true if this is a scalar (non-compound) type.
    fn is_scalar(&self) -> bool {
        !matches!(self, Self::Struct | Self::List | Self::Map)
    }

    fn to_arrow_data_type(&self) -> DataType {
        match self {
            Self::Boolean => DataType::Boolean,
            Self::Int32 => DataType::Int32,
            Self::Int64 => DataType::Int64,
            Self::Float32 => DataType::Float32,
            Self::Float64 => DataType::Float64,
            Self::Utf8 => DataType::Utf8,
            Self::Binary => DataType::Binary,
            Self::TimestampMillisecond => {
                DataType::Timestamp(TimeUnit::Millisecond, Some("+00:00".into()))
            }
            Self::TimestampMicrosecond => {
                DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into()))
            }
            Self::TimestampNanosecond => {
                DataType::Timestamp(TimeUnit::Nanosecond, Some("+00:00".into()))
            }
            Self::Date32 => DataType::Date32,
            // Compound types are handled by resolve_inline_schema via
            // the field's `fields`, `items`, `key_type`, `value_type` properties.
            Self::Struct | Self::List | Self::Map => {
                unreachable!("compound types resolved via inline_field_to_arrow")
            }
        }
    }
}

/// A sub-field definition within a struct type (one level of nesting).
///
/// Sub-fields support only scalar types. For deeper nesting, use
/// `parquet_schema`, `avro_schema`, or `proto_desc_file`.
#[configurable_component]
#[derive(Clone, Debug, PartialEq)]
pub struct ParquetSchemaSubField {
    /// The name of the sub-field.
    #[configurable(metadata(docs::examples = "source", docs::examples = "region"))]
    pub name: String,

    /// The Arrow data type of the sub-field (scalar types only).
    #[serde(rename = "type")]
    #[configurable(metadata(docs::examples = "utf8", docs::examples = "int64"))]
    pub data_type: ParquetFieldType,
}

/// A field definition for the Parquet schema.
#[configurable_component]
#[derive(Clone, Debug, PartialEq)]
pub struct ParquetSchemaField {
    /// The name of the field in the Parquet file.
    #[configurable(metadata(docs::examples = "message", docs::examples = "timestamp"))]
    pub name: String,

    /// The Arrow data type of the field.
    #[serde(rename = "type")]
    #[configurable(metadata(
        docs::examples = "utf8",
        docs::examples = "int64",
        docs::examples = "struct"
    ))]
    pub data_type: ParquetFieldType,

    /// Sub-fields for `struct` type (one level of nesting, scalar types only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[configurable(derived)]
    pub fields: Vec<ParquetSchemaSubField>,

    /// Element type for `list` type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[configurable(derived)]
    pub items: Option<ParquetFieldType>,

    /// Key type for `map` type (must be a string-compatible type).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[configurable(derived)]
    pub key_type: Option<ParquetFieldType>,

    /// Value type for `map` type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[configurable(derived)]
    pub value_type: Option<ParquetFieldType>,
}

/// Configuration for the Parquet serializer.
///
/// Encodes events as Apache Parquet columnar files, optimized for analytical queries
/// via Athena, Trino, Spark, and other columnar query engines.
///
/// Exactly one schema source must be provided. Options (mutually exclusive):
/// - `schema` — inline field list with Vector type names
/// - `parquet_schema` — inline native Parquet message type string
/// - `schema_file` — path to a native Parquet `.schema` file
/// - `avro_schema` — inline Avro JSON schema string
/// - `avro_schema_file` — path to an Avro `.avsc` file
/// - `proto_desc_file` + `proto_message_type` — Protobuf descriptor file
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct ParquetSerializerConfig {
    /// Inline field list defining columns and their Arrow data types.
    ///
    /// For nested types (struct, list, map), use `parquet_schema`, `avro_schema`,
    /// `avro_schema_file`, or `proto_desc_file` instead.
    #[serde(default)]
    #[configurable(derived)]
    pub schema: Vec<ParquetSchemaField>,

    /// Inline native Parquet message type schema string.
    #[serde(default)]
    pub parquet_schema: Option<String>,

    /// Path to a native Parquet schema file.
    #[serde(default)]
    pub schema_file: Option<PathBuf>,

    /// Inline Avro JSON schema string.
    #[serde(default)]
    pub avro_schema: Option<String>,

    /// Path to an Avro schema file (`.avsc`).
    #[serde(default)]
    pub avro_schema_file: Option<PathBuf>,

    /// Path to a Protobuf descriptor file (`.desc`).
    #[serde(default)]
    pub proto_desc_file: Option<PathBuf>,

    /// Protobuf message type name within the descriptor file.
    #[serde(default)]
    pub proto_message_type: Option<String>,

    /// Compression codec applied per column page inside the Parquet file.
    #[serde(default)]
    #[configurable(derived)]
    pub compression: ParquetCompression,

    /// Controls how events with fields not present in the schema are handled.
    #[serde(default)]
    #[configurable(derived)]
    pub schema_mode: SchemaMode,
}

impl ParquetSerializerConfig {
    /// Resolve the Arrow schema from whichever schema source is configured.
    ///
    /// Validates that exactly one schema source is provided and that schema
    /// strings are non-empty.
    fn resolve_schema(&self) -> Result<Schema, Box<dyn std::error::Error + Send + Sync>> {
        let has_inline = !self.schema.is_empty();
        let has_parquet = self.parquet_schema.is_some();
        let has_file = self.schema_file.is_some();
        let has_avro = self.avro_schema.is_some();
        let has_avro_file = self.avro_schema_file.is_some();
        let has_proto = self.proto_desc_file.is_some();

        let count = [
            has_inline,
            has_parquet,
            has_file,
            has_avro,
            has_avro_file,
            has_proto,
        ]
        .iter()
        .filter(|&&b| b)
        .count();

        if count == 0 {
            return Err("Parquet serializer requires a schema with at least one field".into());
        }
        if count > 1 {
            return Err(
                "Schema options are mutually exclusive: only one of schema, parquet_schema, \
                 schema_file, avro_schema, avro_schema_file, or proto_desc_file may be set"
                    .into(),
            );
        }

        if has_inline {
            return self.resolve_inline_schema();
        }
        if has_parquet {
            let s = self.parquet_schema.as_deref().unwrap_or_default();
            if s.trim().is_empty() {
                return Err("parquet_schema is set but empty".into());
            }
            return self.resolve_parquet_schema(s);
        }
        if has_file {
            let path = self.schema_file.as_ref().expect("has_file is true");
            let content = read_schema_file(path, "schema_file")?;
            return self.resolve_parquet_schema(&content);
        }
        if has_avro {
            let s = self.avro_schema.as_deref().unwrap_or_default();
            if s.trim().is_empty() {
                return Err("avro_schema is set but empty".into());
            }
            return self.resolve_avro_schema(s);
        }
        if has_avro_file {
            let path = self
                .avro_schema_file
                .as_ref()
                .expect("has_avro_file is true");
            let content = read_schema_file(path, "avro_schema_file")?;
            return self.resolve_avro_schema(&content);
        }
        if has_proto {
            return self.resolve_proto_schema();
        }

        unreachable!("count >= 1 guarantees at least one branch is taken")
    }

    fn resolve_inline_schema(&self) -> Result<Schema, Box<dyn std::error::Error + Send + Sync>> {
        // Check for duplicate field names
        let mut seen = HashSet::with_capacity(self.schema.len());
        for f in &self.schema {
            if !seen.insert(&f.name) {
                return Err(format!("Duplicate field name in inline schema: '{}'", f.name).into());
            }
        }
        let fields: Vec<Field> = self
            .schema
            .iter()
            .map(|f| inline_field_to_arrow(f))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Schema::new(fields))
    }

    fn resolve_parquet_schema(
        &self,
        schema_str: &str,
    ) -> Result<Schema, Box<dyn std::error::Error + Send + Sync>> {
        let parquet_type = parquet::schema::parser::parse_message_type(schema_str)
            .map_err(|e| format!("Failed to parse Parquet schema: {e}"))?;
        let schema_desc = parquet::schema::types::SchemaDescriptor::new(Arc::new(parquet_type));
        let arrow_schema = parquet::arrow::parquet_to_arrow_schema(&schema_desc, None)
            .map_err(|e| format!("Failed to convert Parquet schema to Arrow: {e}"))?;
        Ok(arrow_schema)
    }

    fn resolve_avro_schema(
        &self,
        avro_str: &str,
    ) -> Result<Schema, Box<dyn std::error::Error + Send + Sync>> {
        let avro_schema = apache_avro::Schema::parse_str(avro_str)
            .map_err(|e| format!("Failed to parse Avro schema: {e}"))?;
        let fields = avro_to_arrow_fields(&avro_schema)?;
        Ok(Schema::new(fields))
    }

    fn resolve_proto_schema(&self) -> Result<Schema, Box<dyn std::error::Error + Send + Sync>> {
        let desc_path = self.proto_desc_file.as_ref().expect("has_proto is true");
        let message_type = self
            .proto_message_type
            .as_deref()
            .ok_or("proto_message_type is required when using proto_desc_file")?;

        let display = desc_path.display();
        let message_descriptor = vrl::protobuf::descriptor::get_message_descriptor(
            desc_path,
            message_type,
        )
        .map_err(|e| {
            format!("Failed to load Protobuf descriptor '{display}' message '{message_type}': {e}")
        })?;

        let fields: Vec<Field> = message_descriptor
            .fields()
            .map(|f| proto_field_to_arrow(&f))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Schema::new(fields))
    }

    /// The data type of events that are accepted by `ParquetSerializer`.
    pub fn input_type(&self) -> vector_core::config::DataType {
        vector_core::config::DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> vector_core::schema::Requirement {
        vector_core::schema::Requirement::empty()
    }
}

/// Read a schema file with size validation (max 10 MB).
/// Convert an inline `ParquetSchemaField` to an Arrow `Field`, handling
/// scalar types directly and compound types (struct, list, map) via their
/// sub-field descriptors. Sub-fields are restricted to scalar types (one
/// level of nesting).
fn inline_field_to_arrow(
    f: &ParquetSchemaField,
) -> Result<Field, Box<dyn std::error::Error + Send + Sync>> {
    match f.data_type {
        ParquetFieldType::Struct => {
            if f.fields.is_empty() {
                return Err(format!(
                    "Field '{}' has type 'struct' but no 'fields' defined",
                    f.name
                )
                .into());
            }
            let sub_fields: Vec<Field> = f
                .fields
                .iter()
                .map(|sf| {
                    if !sf.data_type.is_scalar() {
                        return Err(format!(
                            "Sub-field '{}' in struct '{}' must be a scalar type, got '{:?}'",
                            sf.name, f.name, sf.data_type
                        )
                        .into());
                    }
                    Ok(Field::new(
                        &sf.name,
                        sf.data_type.to_arrow_data_type(),
                        true,
                    ))
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error + Send + Sync>>>()?;
            Ok(Field::new(
                &f.name,
                DataType::Struct(Fields::from(sub_fields)),
                true,
            ))
        }
        ParquetFieldType::List => {
            let items = f.items.as_ref().ok_or_else(|| {
                format!(
                    "Field '{}' has type 'list' but no 'items' type defined",
                    f.name
                )
            })?;
            if !items.is_scalar() {
                return Err(format!(
                    "Field '{}' list 'items' must be a scalar type, got '{items:?}'",
                    f.name
                )
                .into());
            }
            let item_field = Field::new("item", items.to_arrow_data_type(), true);
            Ok(Field::new(
                &f.name,
                DataType::List(Arc::new(item_field)),
                true,
            ))
        }
        ParquetFieldType::Map => {
            let key_type = f.key_type.as_ref().ok_or_else(|| {
                format!(
                    "Field '{}' has type 'map' but no 'key_type' defined",
                    f.name
                )
            })?;
            let value_type = f.value_type.as_ref().ok_or_else(|| {
                format!(
                    "Field '{}' has type 'map' but no 'value_type' defined",
                    f.name
                )
            })?;
            if !key_type.is_scalar() {
                return Err(format!(
                    "Field '{}' map 'key_type' must be a scalar type, got '{key_type:?}'",
                    f.name
                )
                .into());
            }
            if !value_type.is_scalar() {
                return Err(format!(
                    "Field '{}' map 'value_type' must be a scalar type, got '{value_type:?}'",
                    f.name
                )
                .into());
            }
            let entries_field = Field::new(
                "entries",
                DataType::Struct(Fields::from(vec![
                    Field::new("key", key_type.to_arrow_data_type(), false),
                    Field::new("value", value_type.to_arrow_data_type(), true),
                ])),
                false,
            );
            Ok(Field::new(
                &f.name,
                DataType::Map(Arc::new(entries_field), false),
                true,
            ))
        }
        // Scalar types
        _ => Ok(Field::new(&f.name, f.data_type.to_arrow_data_type(), true)),
    }
}

fn read_schema_file(
    path: &std::path::Path,
    field_name: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    const MAX_SCHEMA_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10 MB
    let display = path.display();
    let metadata = std::fs::metadata(path)
        .map_err(|e| format!("Failed to read {field_name} '{display}': {e}"))?;
    if metadata.len() > MAX_SCHEMA_FILE_SIZE {
        return Err(format!(
            "{field_name} '{display}' is too large ({} bytes, max {MAX_SCHEMA_FILE_SIZE})",
            metadata.len()
        )
        .into());
    }
    std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {field_name} '{display}': {e}").into())
}

/// Convert an Avro schema to a list of Arrow fields.
fn avro_to_arrow_fields(
    avro_schema: &apache_avro::Schema,
) -> Result<Vec<Field>, Box<dyn std::error::Error + Send + Sync>> {
    match avro_schema {
        apache_avro::Schema::Record(record) => record
            .fields
            .iter()
            .map(|f| avro_field_to_arrow(&f.name, &f.schema))
            .collect(),
        _ => Err("Avro schema must be a record type".into()),
    }
}

/// Convert a single Avro field to an Arrow field.
fn avro_field_to_arrow(
    name: &str,
    schema: &apache_avro::Schema,
) -> Result<Field, Box<dyn std::error::Error + Send + Sync>> {
    let (data_type, nullable) = avro_type_to_arrow(schema)?;
    Ok(Field::new(name, data_type, nullable))
}

/// Convert an Avro type to an Arrow DataType + nullable flag.
fn avro_type_to_arrow(
    schema: &apache_avro::Schema,
) -> Result<(DataType, bool), Box<dyn std::error::Error + Send + Sync>> {
    match schema {
        apache_avro::Schema::Null => Ok((DataType::Null, true)),
        apache_avro::Schema::Boolean => Ok((DataType::Boolean, true)),
        apache_avro::Schema::Int => Ok((DataType::Int32, true)),
        apache_avro::Schema::Long => Ok((DataType::Int64, true)),
        apache_avro::Schema::Float => Ok((DataType::Float32, true)),
        apache_avro::Schema::Double => Ok((DataType::Float64, true)),
        apache_avro::Schema::String | apache_avro::Schema::Enum(_) => Ok((DataType::Utf8, true)),
        apache_avro::Schema::Bytes | apache_avro::Schema::Fixed(_) => Ok((DataType::Binary, true)),
        apache_avro::Schema::Record(record) => {
            let fields: Vec<Field> = record
                .fields
                .iter()
                .map(|f| avro_field_to_arrow(&f.name, &f.schema))
                .collect::<Result<_, _>>()?;
            Ok((DataType::Struct(fields.into()), true))
        }
        apache_avro::Schema::Array(array_schema) => {
            let (item_type, _) = avro_type_to_arrow(&array_schema.items)?;
            Ok((
                DataType::List(Arc::new(Field::new("item", item_type, true))),
                true,
            ))
        }
        apache_avro::Schema::Map(map_schema) => {
            let (value_type, _) = avro_type_to_arrow(&map_schema.types)?;
            let entries = Field::new(
                "entries",
                DataType::Struct(Fields::from(vec![
                    Field::new("keys", DataType::Utf8, false),
                    Field::new("values", value_type, true),
                ])),
                false,
            );
            Ok((DataType::Map(Arc::new(entries), false), true))
        }
        apache_avro::Schema::Union(union_schema) => {
            // Handle ["null", "type"] pattern → nullable type
            let non_null: Vec<&apache_avro::Schema> = union_schema
                .variants()
                .iter()
                .filter(|s| !matches!(s, apache_avro::Schema::Null))
                .collect();
            if non_null.len() == 1 {
                let (dt, _) = avro_type_to_arrow(non_null[0])?;
                Ok((dt, true))
            } else {
                // Complex union — fall back to Utf8 (data coercion)
                warn!(
                    message = "Complex Avro union mapped to Utf8; multi-type unions are coerced to strings.",
                    variant_count = non_null.len(),
                    internal_log_rate_secs = 30,
                );
                Ok((DataType::Utf8, true))
            }
        }
        apache_avro::Schema::TimestampMillis => Ok((
            DataType::Timestamp(TimeUnit::Millisecond, Some("+00:00".into())),
            true,
        )),
        apache_avro::Schema::TimestampMicros => Ok((
            DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into())),
            true,
        )),
        apache_avro::Schema::Date => Ok((DataType::Date32, true)),
        apache_avro::Schema::Uuid => Ok((DataType::Utf8, true)),
        other => {
            warn!(
                message = "Unmapped Avro type coerced to Binary.",
                avro_type = ?other,
                internal_log_rate_secs = 30,
            );
            Ok((DataType::Binary, true))
        }
    }
}

/// Convert a Protobuf field descriptor to an Arrow field.
fn proto_field_to_arrow(
    field: &prost_reflect::FieldDescriptor,
) -> Result<Field, Box<dyn std::error::Error + Send + Sync>> {
    let arrow_type = if field.is_map() {
        // For map fields, the Kind is Message with is_map_entry() = true
        if let prost_reflect::Kind::Message(msg) = field.kind() {
            let key_field = msg.map_entry_key_field();
            let value_field = msg.map_entry_value_field();
            let key_type = proto_kind_to_arrow(&key_field)?;
            let value_type = proto_kind_to_arrow(&value_field)?;
            let entries = Field::new(
                "entries",
                DataType::Struct(Fields::from(vec![
                    Field::new("keys", key_type, false),
                    Field::new("values", value_type, true),
                ])),
                false,
            );
            DataType::Map(Arc::new(entries), false)
        } else {
            let name = field.name();
            return Err(format!("Map field '{name}' has unexpected kind").into());
        }
    } else if field.is_list() {
        let item_type = proto_kind_to_arrow(field)?;
        DataType::List(Arc::new(Field::new("item", item_type, true)))
    } else {
        proto_kind_to_arrow(field)?
    };
    Ok(Field::new(field.name(), arrow_type, true))
}

/// Convert a Protobuf field's Kind to an Arrow DataType.
fn proto_kind_to_arrow(
    field: &prost_reflect::FieldDescriptor,
) -> Result<DataType, Box<dyn std::error::Error + Send + Sync>> {
    match field.kind() {
        Kind::Double => Ok(DataType::Float64),
        Kind::Float => Ok(DataType::Float32),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => Ok(DataType::Int32),
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => Ok(DataType::Int64),
        Kind::Uint32 | Kind::Fixed32 => Ok(DataType::UInt32),
        Kind::Uint64 | Kind::Fixed64 => Ok(DataType::UInt64),
        Kind::Bool => Ok(DataType::Boolean),
        Kind::String => Ok(DataType::Utf8),
        Kind::Bytes => Ok(DataType::Binary),
        Kind::Enum(_) => Ok(DataType::Utf8),
        Kind::Message(msg) => {
            // Check for well-known Timestamp type
            if msg.full_name() == "google.protobuf.Timestamp" {
                return Ok(DataType::Timestamp(
                    TimeUnit::Nanosecond,
                    Some("+00:00".into()),
                ));
            }
            let fields: Vec<Field> = msg
                .fields()
                .map(|f| proto_field_to_arrow(&f))
                .collect::<Result<_, _>>()?;
            Ok(DataType::Struct(fields.into()))
        }
    }
}

/// Check the resolved Arrow schema for data types unsupported by the JSON-based
/// encode path (`arrow::json::reader::ReaderBuilder`). Binary variants are
/// accepted by Parquet/Arrow at the schema level but the JSON decoder rejects
/// them at runtime, so we fail fast here at config time.
///
/// This walks the full field tree (including nested structs, lists, and map
/// values) so it catches binary fields regardless of schema source (inline,
/// Avro `bytes`/`fixed`, Protobuf `bytes`, or native Parquet `BYTE_ARRAY`
/// without a STRING annotation).
fn reject_unsupported_arrow_types(
    schema: &Schema,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    fn check_field(
        field: &Field,
        path: &str,
        bad: &mut Vec<String>,
    ) {
        let name = if path.is_empty() {
            field.name().to_string()
        } else {
            format!("{path}.{}", field.name())
        };
        match field.data_type() {
            DataType::Binary | DataType::LargeBinary | DataType::FixedSizeBinary(_) => {
                bad.push(format!("'{name}' ({:?})", field.data_type()));
            }
            DataType::Struct(fields) => {
                for f in fields {
                    check_field(f, &name, bad);
                }
            }
            DataType::List(inner) | DataType::LargeList(inner) => {
                check_field(inner, &name, bad);
            }
            DataType::Map(entries_field, _) => {
                // Map entries is a struct with key + value; check value field
                if let DataType::Struct(kv) = entries_field.data_type() {
                    for f in kv {
                        check_field(f, &name, bad);
                    }
                }
            }
            _ => {}
        }
    }

    let mut bad = Vec::new();
    for field in schema.fields() {
        check_field(field, "", &mut bad);
    }
    if !bad.is_empty() {
        return Err(format!(
            "Schema contains binary field(s) unsupported by the JSON-based Arrow encoder: {}. \
             Use Utf8 for base64/hex-encoded data instead.",
            bad.join(", ")
        )
        .into());
    }
    Ok(())
}

/// Parquet batch serializer.
#[derive(Clone, Debug)]
pub struct ParquetSerializer {
    schema: SchemaRef,
    writer_props: Arc<WriterProperties>,
    schema_mode: SchemaMode,
    /// Pre-built set of schema field names for O(1) strict-mode lookups.
    schema_field_names: HashSet<String>,
}

impl ParquetSerializer {
    /// Create a new `ParquetSerializer` from the given configuration.
    pub fn new(
        config: ParquetSerializerConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let schema = config.resolve_schema()?;
        reject_unsupported_arrow_types(&schema)?;
        let schema_ref = SchemaRef::new(schema);

        let schema_field_names = schema_ref
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect::<HashSet<_>>();

        let writer_props = Arc::new(
            WriterProperties::builder()
                .set_compression(config.compression.to_parquet_compression())
                .build(),
        );

        Ok(Self {
            schema: schema_ref,
            writer_props,
            schema_mode: config.schema_mode,
            schema_field_names,
        })
    }

    /// Returns the MIME content type for Parquet data.
    pub const fn content_type(&self) -> &'static str {
        "application/vnd.apache.parquet"
    }
}

impl tokio_util::codec::Encoder<Vec<Event>> for ParquetSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, events: Vec<Event>, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        if events.is_empty() {
            return Ok(());
        }

        // Warn about non-log events that will be silently dropped by the Arrow layer.
        // Parquet encoding only supports Log events (declared via input_type).
        let non_log_count = events.iter().filter(|e| e.maybe_as_log().is_none()).count();
        if non_log_count > 0 {
            warn!(
                message = "Non-log events dropped by Parquet encoder.",
                %non_log_count,
                internal_log_rate_secs = 10,
            );
        }

        // In strict mode, check for extra top-level fields not in the schema
        if self.schema_mode == SchemaMode::Strict {
            for event in &events {
                if let Some(log) = event.maybe_as_log()
                    && let Some(fields) = log.all_event_fields()
                {
                    for (key, _) in fields {
                        // Extract only the top-level field name (before first '.' or '[')
                        let field_name = key.strip_prefix('.').unwrap_or(&key);
                        let top_level = field_name
                            .find(['.', '['])
                            .map(|pos| &field_name[..pos])
                            .unwrap_or(field_name);
                        if !self.schema_field_names.contains(top_level) {
                            return Err(Box::new(ArrowEncodingError::SchemaFetchError {
                                message: format!(
                                    "Strict schema mode: event contains field '{top_level}' not in schema",
                                ),
                            }));
                        }
                    }
                }
            }
        }

        // Build RecordBatch using the shared Arrow logic
        let record_batch = build_record_batch(Arc::clone(&self.schema), &events)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Write Parquet directly into the output buffer (no intermediate Vec)
        let mut writer = ArrowWriter::try_new(
            buffer.writer(),
            Arc::clone(record_batch.schema_ref()),
            Some((*self.writer_props).clone()),
        )?;
        writer.write(&record_batch)?;
        writer.close()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use parquet::file::reader::{FileReader, SerializedFileReader};
    use parquet::record::reader::RowIter;
    use tokio_util::codec::Encoder;
    use vector_core::event::LogEvent;

    /// Helper to create a simple log event from key-value pairs
    fn create_event<V>(fields: Vec<(&str, V)>) -> Event
    where
        V: Into<vector_core::event::Value>,
    {
        let mut log = LogEvent::default();
        for (key, value) in fields {
            log.insert(key, value.into());
        }
        Event::Log(log)
    }

    /// Helper to create a Vec<ParquetSchemaField> from (name, type) pairs
    fn schema_fields(fields: Vec<(&str, &str)>) -> Vec<ParquetSchemaField> {
        fields
            .into_iter()
            .map(|(name, typ)| ParquetSchemaField {
                name: name.to_string(),
                data_type: serde_json::from_value(serde_json::json!(typ))
                    .unwrap_or_else(|e| panic!("Invalid type '{}': {}", typ, e)),
                fields: Vec::new(),
                items: None,
                key_type: None,
                value_type: None,
            })
            .collect()
    }

    /// Helper to build a ParquetSerializer with given fields and defaults
    fn make_serializer(fields: Vec<(&str, &str)>) -> ParquetSerializer {
        ParquetSerializer::new(ParquetSerializerConfig {
            schema: schema_fields(fields),
            ..Default::default()
        })
        .expect("Failed to create serializer")
    }

    /// Verify the output bytes start with the Parquet magic number "PAR1"
    fn assert_parquet_magic(data: &[u8]) {
        assert!(data.len() >= 4, "Output too short to be valid Parquet");
        assert_eq!(&data[..4], b"PAR1", "Missing Parquet magic bytes");
    }

    /// Read a Parquet file from bytes and return the row count
    fn parquet_row_count(data: &[u8]) -> usize {
        let reader =
            SerializedFileReader::new(Bytes::copy_from_slice(data)).expect("Invalid Parquet file");
        let iter = RowIter::from_file_into(Box::new(reader));
        iter.count()
    }

    /// Read a Parquet file and return column names from the schema
    fn parquet_column_names(data: &[u8]) -> Vec<String> {
        let reader =
            SerializedFileReader::new(Bytes::copy_from_slice(data)).expect("Invalid Parquet file");
        let schema = reader.metadata().file_metadata().schema_descr();
        schema
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect()
    }

    #[test]
    fn test_parquet_encode_basic() {
        use vector_core::event::Value;

        let mut serializer = make_serializer(vec![("name", "utf8"), ("age", "int64")]);

        let mut log1 = LogEvent::default();
        log1.insert("name", "alice");
        log1.insert("age", Value::Integer(30));

        let mut log2 = LogEvent::default();
        log2.insert("name", "bob");
        log2.insert("age", Value::Integer(25));

        let events = vec![Event::Log(log1), Event::Log(log2)];

        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("Encoding should succeed");

        let data = buffer.freeze();
        assert_parquet_magic(&data);

        // Verify row count
        assert_eq!(parquet_row_count(&data), 2);

        // Verify schema columns
        let columns = parquet_column_names(&data);
        assert_eq!(columns, vec!["name", "age"]);
    }

    #[test]
    fn test_parquet_relaxed_mode_missing_fields() {
        let mut serializer = make_serializer(vec![("name", "utf8"), ("age", "int64")]);

        // Event only has "name", missing "age"
        let events = vec![create_event(vec![("name", "alice")])];

        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("Relaxed mode should handle missing fields");

        let data = buffer.freeze();
        assert_parquet_magic(&data);
        assert_eq!(parquet_row_count(&data), 1);
    }

    #[test]
    fn test_parquet_relaxed_mode_extra_fields() {
        let mut serializer = make_serializer(vec![("name", "utf8")]);

        // Event has "name" + extra "city" field not in schema
        let events = vec![create_event(vec![("name", "alice"), ("city", "paris")])];

        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("Relaxed mode should drop extra fields");

        let data = buffer.freeze();
        assert_parquet_magic(&data);
        assert_eq!(parquet_row_count(&data), 1);

        // Schema should only have "name"
        let columns = parquet_column_names(&data);
        assert_eq!(columns, vec!["name"]);
    }

    #[test]
    fn test_parquet_strict_mode_extra_fields_error() {
        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema: schema_fields(vec![("name", "utf8")]),
            schema_mode: SchemaMode::Strict,
            ..Default::default()
        })
        .expect("Failed to create serializer");

        // Event has extra "city" field not in schema
        let events = vec![create_event(vec![("name", "alice"), ("city", "paris")])];

        let mut buffer = BytesMut::new();
        let result = serializer.encode(events, &mut buffer);
        assert!(result.is_err(), "Strict mode should reject extra fields");

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("city"),
            "Error should mention the extra field name, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_parquet_compression_variants() {
        let events = vec![create_event(vec![("msg", "hello world")])];

        let compressions = vec![
            ParquetCompression::None,
            ParquetCompression::Snappy,
            ParquetCompression::Zstd,
            ParquetCompression::Gzip,
            ParquetCompression::Lz4,
        ];

        for compression in compressions {
            let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
                schema: schema_fields(vec![("msg", "utf8")]),
                compression,
                ..Default::default()
            })
            .expect("Failed to create serializer");

            let mut buffer = BytesMut::new();
            serializer
                .encode(events.clone(), &mut buffer)
                .unwrap_or_else(|e| panic!("Encoding with {:?} failed: {}", compression, e));

            let data = buffer.freeze();
            assert_parquet_magic(&data);
            assert_eq!(
                parquet_row_count(&data),
                1,
                "Wrong row count for {:?}",
                compression
            );
        }
    }

    #[test]
    fn test_parquet_empty_events() {
        let mut serializer = make_serializer(vec![("msg", "utf8")]);

        let events: Vec<Event> = vec![];
        let mut buffer = BytesMut::new();

        serializer
            .encode(events, &mut buffer)
            .expect("Empty events should succeed");

        assert!(buffer.is_empty(), "Buffer should be empty for empty events");
    }

    #[test]
    fn test_parquet_config_deserialization() {
        let json = serde_json::json!({
            "schema": [
                {"name": "timestamp", "type": "timestamp_millisecond"},
                {"name": "message", "type": "utf8"},
                {"name": "level", "type": "utf8"},
                {"name": "count", "type": "int64"},
                {"name": "ratio", "type": "float64"}
            ],
            "compression": "zstd",
            "schema_mode": "strict"
        });

        let config: ParquetSerializerConfig =
            serde_json::from_value(json).expect("Config should deserialize");

        assert_eq!(config.schema.len(), 5);
        assert_eq!(config.schema[0].name, "timestamp");
        assert_eq!(
            config.schema[0].data_type,
            ParquetFieldType::TimestampMillisecond
        );
        assert_eq!(config.compression, ParquetCompression::Zstd);
        assert_eq!(config.schema_mode, SchemaMode::Strict);

        // Verify the config can build a working serializer
        let serializer = ParquetSerializer::new(config);
        assert!(
            serializer.is_ok(),
            "Should build serializer from deserialized config"
        );
    }

    #[test]
    fn test_parquet_empty_schema_error() {
        let config = ParquetSerializerConfig::default();
        let result = ParquetSerializer::new(config);
        assert!(result.is_err(), "Should fail when schema has no fields");
    }

    #[test]
    fn test_parquet_schema_field_names_prebuilt() {
        let serializer = make_serializer(vec![
            ("message", "utf8"),
            ("host", "utf8"),
            ("status", "int64"),
        ]);

        // Verify the HashSet was correctly populated at construction time
        assert_eq!(serializer.schema_field_names.len(), 3);
        assert!(serializer.schema_field_names.contains("message"));
        assert!(serializer.schema_field_names.contains("host"));
        assert!(serializer.schema_field_names.contains("status"));
        assert!(!serializer.schema_field_names.contains("nonexistent"));
    }

    #[test]
    fn test_parquet_strict_mode_uses_hashset_lookup() {
        // Strict mode should use the pre-built HashSet for O(1) field validation
        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema: schema_fields(vec![("name", "utf8"), ("age", "int64")]),
            schema_mode: SchemaMode::Strict,
            ..Default::default()
        })
        .expect("Failed to create serializer");

        // Valid event - all fields in schema
        let valid_events = vec![create_event(vec![("name", "alice")])];
        let mut buffer = BytesMut::new();
        assert!(
            serializer.encode(valid_events, &mut buffer).is_ok(),
            "Strict mode should accept events with only schema fields"
        );

        // Invalid event - extra field
        let invalid_events = vec![create_event(vec![
            ("name", "bob"),
            ("unknown_field", "value"),
        ])];
        let mut buffer = BytesMut::new();
        let result = serializer.encode(invalid_events, &mut buffer);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("unknown_field"),
            "Error should reference the extra field"
        );
    }

    #[test]
    fn test_parquet_writer_props_arc_shared() {
        // Verify WriterProperties is wrapped in Arc (clone is cheap)
        let serializer = make_serializer(vec![("msg", "utf8")]);
        let cloned = serializer.clone();

        // Arc::strong_count should be 2 after clone
        assert_eq!(
            Arc::strong_count(&serializer.writer_props),
            2,
            "WriterProperties should be shared via Arc"
        );
        drop(cloned);
        assert_eq!(Arc::strong_count(&serializer.writer_props), 1);
    }

    #[test]
    fn test_parquet_direct_buffer_write() {
        // Verify encode writes directly to buffer (not double-buffered)
        let mut serializer = make_serializer(vec![("msg", "utf8")]);

        let events = vec![create_event(vec![("msg", "test")])];
        let mut buffer = BytesMut::new();

        serializer
            .encode(events, &mut buffer)
            .expect("Encoding should succeed");

        // Buffer should contain valid Parquet data directly
        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    #[test]
    fn test_parquet_schema_already_nullable() {
        // Verify schema fields are nullable without redundant transformation
        let serializer = make_serializer(vec![("name", "utf8"), ("count", "int64")]);

        for field in serializer.schema.fields() {
            assert!(
                field.is_nullable(),
                "Field '{}' should be nullable",
                field.name()
            );
        }
    }

    #[test]
    fn test_parquet_multiple_batches_same_serializer() {
        // Verify serializer can encode multiple batches correctly (Arc<WriterProperties> reuse)
        let mut serializer = make_serializer(vec![("msg", "utf8")]);

        for i in 0..3 {
            let events = vec![create_event(vec![("msg", format!("batch_{}", i))])];
            let mut buffer = BytesMut::new();

            serializer
                .encode(events, &mut buffer)
                .unwrap_or_else(|e| panic!("Batch {} failed: {}", i, e));

            assert_parquet_magic(&buffer);
            assert_eq!(parquet_row_count(&buffer), 1, "Batch {} wrong row count", i);
        }
    }

    // ========================================================================
    // Schema Option #2: Nested types via Avro schema (struct, list, map)
    // ========================================================================

    #[test]
    fn test_avro_schema_nested_list() {
        use vector_core::event::Value;

        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "test",
                "fields": [
                    {"name": "tags", "type": {"type": "array", "items": "string"}}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let mut serializer =
            ParquetSerializer::new(config).expect("Should create serializer with list field");

        let mut log = LogEvent::default();
        log.insert(
            "tags",
            Value::Array(vec![
                Value::Bytes("tag1".into()),
                Value::Bytes("tag2".into()),
            ]),
        );

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Encoding list field should succeed");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    #[test]
    fn test_avro_schema_nested_map() {
        use vector_core::event::Value;
        use vrl::value::ObjectMap;

        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "test",
                "fields": [
                    {"name": "labels", "type": {"type": "map", "values": "string"}}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let mut serializer =
            ParquetSerializer::new(config).expect("Should create serializer with map field");

        let mut labels = ObjectMap::new();
        labels.insert("env".into(), Value::Bytes("prod".into()));
        labels.insert("region".into(), Value::Bytes("us-east-1".into()));

        let mut log = LogEvent::default();
        log.insert("labels", Value::Object(labels));

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Encoding map field should succeed");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    // ========================================================================
    // Schema Option #3: Native Parquet schema inline
    // ========================================================================

    #[test]
    fn test_parquet_schema_inline() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "parquet_schema": "message logs {\n  required binary timestamp (STRING);\n  required binary message (STRING);\n  optional int64 count;\n}"
        }))
        .expect("Config should deserialize");

        let mut serializer = ParquetSerializer::new(config)
            .expect("Should create serializer from native Parquet schema");

        let mut log = LogEvent::default();
        log.insert("timestamp", "2024-01-01T00:00:00Z");
        log.insert("message", "hello");

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Encoding with Parquet schema should succeed");

        let data = buffer.freeze();
        assert_parquet_magic(&data);
        assert_eq!(parquet_row_count(&data), 1);

        let columns = parquet_column_names(&data);
        assert!(columns.contains(&"timestamp".to_string()));
        assert!(columns.contains(&"message".to_string()));
        assert!(columns.contains(&"count".to_string()));
    }

    // ========================================================================
    // Schema Option #4: Native Parquet schema from file
    // ========================================================================

    #[test]
    fn test_parquet_schema_file() {
        use std::io::Write;
        let schema_path =
            std::env::temp_dir().join(format!("vector_test_parquet_{}.schema", std::process::id()));
        let mut f = std::fs::File::create(&schema_path).expect("Failed to create schema file");
        write!(
            f,
            "message logs {{\n  required binary name (STRING);\n  optional int64 age;\n}}"
        )
        .expect("Failed to write schema");

        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema_file": schema_path.to_str().unwrap()
        }))
        .expect("Config should deserialize");

        let mut serializer =
            ParquetSerializer::new(config).expect("Should create serializer from schema file");

        let mut log = LogEvent::default();
        log.insert("name", "alice");

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Encoding with schema file should succeed");

        let data = buffer.freeze();
        assert_parquet_magic(&data);
        assert_eq!(parquet_row_count(&data), 1);

        let columns = parquet_column_names(&data);
        assert_eq!(columns, vec!["name", "age"]);
    }

    // ========================================================================
    // Schema Option #5: Avro schema inline
    // ========================================================================

    #[test]
    fn test_avro_schema_inline() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "logs",
                "fields": [
                    {"name": "message", "type": "string"},
                    {"name": "level", "type": "string"},
                    {"name": "count", "type": ["null", "long"]}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let mut serializer =
            ParquetSerializer::new(config).expect("Should create serializer from Avro schema");

        let mut log = LogEvent::default();
        log.insert("message", "hello");
        log.insert("level", "INFO");

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Encoding with Avro schema should succeed");

        let data = buffer.freeze();
        assert_parquet_magic(&data);
        assert_eq!(parquet_row_count(&data), 1);

        let columns = parquet_column_names(&data);
        assert!(columns.contains(&"message".to_string()));
        assert!(columns.contains(&"level".to_string()));
        assert!(columns.contains(&"count".to_string()));
    }

    #[test]
    fn test_avro_schema_nullable_union() {
        // Avro ["null", "string"] should become a nullable Utf8 Arrow field
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "test",
                "fields": [
                    {"name": "required_field", "type": "string"},
                    {"name": "optional_field", "type": ["null", "string"]}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let mut serializer = ParquetSerializer::new(config)
            .expect("Should create serializer from Avro schema with nullable union");

        // Event with only required field, optional is missing → null
        let mut log = LogEvent::default();
        log.insert("required_field", "value");

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Encoding with nullable union should succeed");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    #[test]
    fn test_avro_schema_nested_record() {
        use vector_core::event::Value;
        use vrl::value::ObjectMap;

        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "logs",
                "fields": [
                    {"name": "message", "type": "string"},
                    {"name": "metadata", "type": {
                        "type": "record",
                        "name": "metadata_record",
                        "fields": [
                            {"name": "request_id", "type": "string"},
                            {"name": "duration_ms", "type": "long"}
                        ]
                    }}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let mut serializer = ParquetSerializer::new(config)
            .expect("Should create serializer from Avro nested record");

        let mut metadata = ObjectMap::new();
        metadata.insert("request_id".into(), Value::Bytes("req-456".into()));
        metadata.insert("duration_ms".into(), Value::Integer(100));

        let mut log = LogEvent::default();
        log.insert("message", "test");
        log.insert("metadata", Value::Object(metadata));

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Encoding with Avro nested record should succeed");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    // ========================================================================
    // Schema Option #6: Avro schema from file (.avsc)
    // ========================================================================

    #[test]
    fn test_avro_schema_file() {
        use std::io::Write;
        let avsc_path =
            std::env::temp_dir().join(format!("vector_test_parquet_{}.avsc", std::process::id()));
        let mut f = std::fs::File::create(&avsc_path).expect("Failed to create avsc file");
        write!(
            f,
            r#"{{"type": "record", "name": "logs", "fields": [{{"name": "msg", "type": "string"}}, {{"name": "severity", "type": "int"}}]}}"#
        )
        .expect("Failed to write avsc");

        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema_file": avsc_path.to_str().unwrap()
        }))
        .expect("Config should deserialize");

        let mut serializer =
            ParquetSerializer::new(config).expect("Should create serializer from Avro schema file");

        let mut log = LogEvent::default();
        log.insert("msg", "hello");

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Encoding with Avro schema file should succeed");

        let data = buffer.freeze();
        assert_parquet_magic(&data);
        assert_eq!(parquet_row_count(&data), 1);

        let columns = parquet_column_names(&data);
        assert!(columns.contains(&"msg".to_string()));
        assert!(columns.contains(&"severity".to_string()));
    }

    // ========================================================================
    // Schema Option #7: Protobuf descriptor file
    // ========================================================================

    #[test]
    fn test_proto_desc_file() {
        // Use a test descriptor file if available, otherwise create minimal one
        // For now, test the config deserialization and validation path
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "proto_desc_file": "/nonexistent/test.desc",
            "proto_message_type": "test.LogRecord"
        }))
        .expect("Config should deserialize");

        // This should fail because the file doesn't exist
        let result = ParquetSerializer::new(config);
        assert!(result.is_err(), "Should fail when desc file doesn't exist");
    }

    // ========================================================================
    // Schema mutual exclusion validation
    // ========================================================================

    #[test]
    fn test_schema_mutual_exclusion_error() {
        // Setting both schema and parquet_schema should error
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [{"name": "msg", "type": "utf8"}],
            "parquet_schema": "message logs { required binary msg (STRING); }"
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(
            result.is_err(),
            "Should reject config with multiple schema sources"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("mutually exclusive") || err.contains("only one"),
            "Error should mention mutual exclusion, got: {}",
            err
        );
    }

    #[test]
    fn test_no_schema_specified_error() {
        // No schema option set at all
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "compression": "snappy"
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err(), "Should fail when no schema is specified");
    }

    // ========================================================================
    // Type mapping tests
    // ========================================================================

    #[test]
    fn test_duplicate_field_names_error() {
        let config = ParquetSerializerConfig {
            schema: schema_fields(vec![("msg", "utf8"), ("msg", "int64")]),
            ..Default::default()
        };

        let result = ParquetSerializer::new(config);
        assert!(result.is_err(), "Should reject duplicate field names");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Duplicate") && err.contains("msg"),
            "Error should mention duplicate field name, got: {err}",
        );
    }

    #[test]
    fn test_empty_parquet_schema_string_error() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "parquet_schema": ""
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err(), "Should reject empty parquet_schema");
        assert!(
            result.unwrap_err().to_string().contains("empty"),
            "Error should mention empty schema"
        );
    }

    #[test]
    fn test_empty_avro_schema_string_error() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": "  "
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err(), "Should reject whitespace-only avro_schema");
        assert!(
            result.unwrap_err().to_string().contains("empty"),
            "Error should mention empty schema"
        );
    }

    #[test]
    fn test_avro_to_arrow_type_mapping() {
        // Test supported Avro primitive types map correctly.
        // Note: Avro "bytes" maps to Arrow Binary which is rejected by
        // reject_unsupported_arrow_types — tested separately in
        // test_avro_bytes_rejected_at_config_time.
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "all_types",
                "fields": [
                    {"name": "f_bool", "type": "boolean"},
                    {"name": "f_int", "type": "int"},
                    {"name": "f_long", "type": "long"},
                    {"name": "f_float", "type": "float"},
                    {"name": "f_double", "type": "double"},
                    {"name": "f_string", "type": "string"}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let serializer =
            ParquetSerializer::new(config).expect("Should create serializer with Avro types");

        let fields = serializer.schema.fields();
        assert_eq!(fields.len(), 6);
        assert_eq!(*fields[0].data_type(), DataType::Boolean);
        assert_eq!(*fields[1].data_type(), DataType::Int32);
        assert_eq!(*fields[2].data_type(), DataType::Int64);
        assert_eq!(*fields[3].data_type(), DataType::Float32);
        assert_eq!(*fields[4].data_type(), DataType::Float64);
        assert_eq!(*fields[5].data_type(), DataType::Utf8);
    }

    // ========================================================================
    // Edge cases: Inline schema
    // ========================================================================

    #[test]
    fn test_inline_all_data_types() {
        use vector_core::event::Value;

        let all_types = vec![
            ("f_bool", "boolean"),
            ("f_i32", "int32"),
            ("f_i64", "int64"),
            ("f_f32", "float32"),
            ("f_f64", "float64"),
            ("f_utf8", "utf8"),
            ("f_ts_ms", "timestamp_millisecond"),
            ("f_ts_us", "timestamp_microsecond"),
            ("f_ts_ns", "timestamp_nanosecond"),
            ("f_date", "date32"),
        ];

        let mut serializer = make_serializer(all_types);

        let mut log = LogEvent::default();
        log.insert("f_bool", Value::Boolean(true));
        log.insert("f_i32", Value::Integer(42));
        log.insert("f_i64", Value::Integer(123456789));
        log.insert(
            "f_f32",
            Value::Float(ordered_float::NotNan::new(1.23).unwrap()),
        );
        log.insert(
            "f_f64",
            Value::Float(ordered_float::NotNan::new(9.81).unwrap()),
        );
        log.insert("f_utf8", "hello");
        // Timestamp/date fields left null — Arrow builder doesn't support "UTC"
        // timezone string without chrono-tz, so we verify schema creation works
        // and null handling is correct.

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Encoding all data types (with null timestamps) should succeed");

        let data = buffer.freeze();
        assert_parquet_magic(&data);
        assert_eq!(parquet_row_count(&data), 1);

        let columns = parquet_column_names(&data);
        assert_eq!(columns.len(), 10);
    }

    #[test]
    fn test_inline_unicode_field_names() {
        let mut serializer = make_serializer(vec![("名前", "utf8"), ("данные", "int64")]);

        let mut log = LogEvent::default();
        log.insert("名前", "太郎");
        log.insert("данные", vector_core::event::Value::Integer(99));

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Unicode field names should work");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    #[test]
    fn test_inline_all_fields_missing() {
        // All schema fields missing from event → all nulls
        let mut serializer = make_serializer(vec![("a", "utf8"), ("b", "int64"), ("c", "float64")]);

        let log = LogEvent::default();
        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("All-null row should succeed");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    #[test]
    fn test_inline_large_batch() {
        use vector_core::event::Value;

        let mut serializer = make_serializer(vec![("id", "int64"), ("msg", "utf8")]);

        let events: Vec<Event> = (0..1000)
            .map(|i| {
                let mut log = LogEvent::default();
                log.insert("id", Value::Integer(i));
                log.insert("msg", format!("message_{i}"));
                Event::Log(log)
            })
            .collect();

        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("Large batch should succeed");

        let data = buffer.freeze();
        assert_parquet_magic(&data);
        assert_eq!(parquet_row_count(&data), 1000);
    }

    #[test]
    fn test_inline_single_field_schema() {
        let mut serializer = make_serializer(vec![("only", "utf8")]);

        let events = vec![create_event(vec![("only", "value")])];
        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("Single-field schema should work");

        assert_parquet_magic(&buffer);
        let columns = parquet_column_names(&buffer);
        assert_eq!(columns, vec!["only"]);
    }

    // ========================================================================
    // Edge cases: Parquet native schema
    // ========================================================================

    #[test]
    fn test_parquet_schema_with_nested_group() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "parquet_schema": "message logs {\n  required binary name (STRING);\n  optional group address {\n    optional binary city (STRING);\n    optional binary zip (STRING);\n  }\n}"
        }))
        .expect("Config should deserialize");

        let serializer =
            ParquetSerializer::new(config).expect("Parquet schema with nested group should parse");

        // Verify schema has the expected structure
        assert_eq!(serializer.schema.fields().len(), 2);
        assert_eq!(serializer.schema.field(0).name(), "name");
        assert_eq!(serializer.schema.field(1).name(), "address");
        assert!(
            matches!(serializer.schema.field(1).data_type(), DataType::Struct(_)),
            "address should be a struct"
        );
    }

    #[test]
    fn test_parquet_schema_with_repeated_field() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "parquet_schema": "message logs {\n  required binary name (STRING);\n  repeated binary tags (STRING);\n}"
        }))
        .expect("Config should deserialize");

        let serializer = ParquetSerializer::new(config)
            .expect("Parquet schema with repeated field should parse");

        assert_eq!(serializer.schema.fields().len(), 2);
    }

    #[test]
    fn test_parquet_schema_invalid_syntax_error() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "parquet_schema": "this is not valid parquet schema syntax !!!"
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err(), "Invalid Parquet schema should error");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse Parquet schema"),
            "Error should mention parsing failure"
        );
    }

    #[test]
    fn test_parquet_schema_file_not_found_error() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema_file": "/nonexistent/path/schema.parquet"
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err(), "Missing schema file should error");
        assert!(
            result.unwrap_err().to_string().contains("Failed to read"),
            "Error should mention file read failure"
        );
    }

    // ========================================================================
    // Edge cases: Avro schema
    // ========================================================================

    #[test]
    fn test_avro_schema_invalid_json_error() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": "{ not valid json }"
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err(), "Invalid Avro JSON should error");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse Avro schema"),
            "Error should mention Avro parsing failure"
        );
    }

    #[test]
    fn test_avro_schema_non_record_error() {
        // Top-level Avro type must be a record
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#""string""#
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err(), "Non-record Avro schema should error");
        assert!(
            result.unwrap_err().to_string().contains("record"),
            "Error should mention record type requirement"
        );
    }

    #[test]
    fn test_avro_schema_enum_type() {
        // Avro enum should map to Utf8
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "test",
                "fields": [
                    {"name": "severity", "type": {
                        "type": "enum",
                        "name": "Severity",
                        "symbols": ["DEBUG", "INFO", "WARN", "ERROR"]
                    }}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let serializer = ParquetSerializer::new(config).expect("Avro enum schema should parse");

        assert_eq!(*serializer.schema.field(0).data_type(), DataType::Utf8);
    }

    #[test]
    fn test_avro_schema_timestamp_logical_types() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "test",
                "fields": [
                    {"name": "ts_ms", "type": {"type": "long", "logicalType": "timestamp-millis"}},
                    {"name": "ts_us", "type": {"type": "long", "logicalType": "timestamp-micros"}},
                    {"name": "d", "type": {"type": "int", "logicalType": "date"}}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let serializer =
            ParquetSerializer::new(config).expect("Avro timestamp logical types should parse");

        let fields = serializer.schema.fields();
        assert_eq!(
            *fields[0].data_type(),
            DataType::Timestamp(TimeUnit::Millisecond, Some("+00:00".into()))
        );
        assert_eq!(
            *fields[1].data_type(),
            DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into()))
        );
        assert_eq!(*fields[2].data_type(), DataType::Date32);
    }

    #[test]
    fn test_avro_schema_deeply_nested() {
        use vector_core::event::Value;
        use vrl::value::ObjectMap;

        // Record with nested record with nested record
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "root",
                "fields": [
                    {"name": "level1", "type": {
                        "type": "record",
                        "name": "level1_rec",
                        "fields": [
                            {"name": "level2", "type": {
                                "type": "record",
                                "name": "level2_rec",
                                "fields": [
                                    {"name": "value", "type": "string"}
                                ]
                            }}
                        ]
                    }}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let mut serializer =
            ParquetSerializer::new(config).expect("Deeply nested Avro should parse");

        let mut level2 = ObjectMap::new();
        level2.insert("value".into(), Value::Bytes("deep".into()));

        let mut level1 = ObjectMap::new();
        level1.insert("level2".into(), Value::Object(level2));

        let mut log = LogEvent::default();
        log.insert("level1", Value::Object(level1));

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Deeply nested encoding should succeed");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    #[test]
    fn test_avro_schema_array_of_records() {
        // Array containing records (nested complex type)
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "test",
                "fields": [
                    {"name": "items", "type": {
                        "type": "array",
                        "items": {
                            "type": "record",
                            "name": "item_rec",
                            "fields": [
                                {"name": "name", "type": "string"},
                                {"name": "qty", "type": "int"}
                            ]
                        }
                    }}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let serializer =
            ParquetSerializer::new(config).expect("Array of records schema should parse");

        let item_type = serializer.schema.field(0).data_type();
        assert!(
            matches!(item_type, DataType::List(_)),
            "Should be a List type, got {item_type:?}"
        );
    }

    #[test]
    fn test_avro_schema_file_not_found_error() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema_file": "/nonexistent/missing.avsc"
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to read"));
    }

    // ========================================================================
    // Edge cases: Protobuf schema
    // ========================================================================

    #[test]
    fn test_proto_missing_message_type_error() {
        // proto_desc_file without proto_message_type should error
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "proto_desc_file": "/some/file.desc"
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("proto_message_type is required"),
            "Error should mention missing proto_message_type"
        );
    }

    #[test]
    fn test_proto_message_type_without_desc_file_ignored() {
        // proto_message_type alone (without proto_desc_file) should be
        // silently ignored — not counted as a schema source
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [{"name": "msg", "type": "utf8"}],
            "proto_message_type": "unused.Message"
        }))
        .expect("Config should deserialize");

        // Should succeed using the inline schema, ignoring proto_message_type
        let serializer = ParquetSerializer::new(config);
        assert!(
            serializer.is_ok(),
            "proto_message_type without proto_desc_file should be ignored"
        );
    }

    // ========================================================================
    // Edge cases: Mutual exclusion combinations
    // ========================================================================

    #[test]
    fn test_schema_mutual_exclusion_inline_and_avro() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [{"name": "msg", "type": "utf8"}],
            "avro_schema": r#"{"type":"record","name":"t","fields":[{"name":"x","type":"string"}]}"#
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("mutually exclusive")
        );
    }

    #[test]
    fn test_schema_mutual_exclusion_file_and_avro_file() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema_file": "/some/schema.parquet",
            "avro_schema_file": "/some/schema.avsc"
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("mutually exclusive")
        );
    }

    #[test]
    fn test_schema_mutual_exclusion_three_sources() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [{"name": "a", "type": "utf8"}],
            "parquet_schema": "message t { required binary a (STRING); }",
            "avro_schema": r#"{"type":"record","name":"t","fields":[{"name":"a","type":"string"}]}"#
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("mutually exclusive")
        );
    }

    // ========================================================================
    // Edge cases: Encoding behavior
    // ========================================================================

    #[test]
    fn test_strict_mode_allows_schema_fields() {
        // Strict mode should pass when all event fields are in the schema
        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema: schema_fields(vec![("name", "utf8"), ("level", "utf8")]),
            schema_mode: SchemaMode::Strict,
            ..Default::default()
        })
        .expect("Failed to create serializer");

        let mut log = LogEvent::default();
        log.insert("name", "test");
        log.insert("level", "info");

        let mut buffer = BytesMut::new();
        assert!(
            serializer
                .encode(vec![Event::Log(log)], &mut buffer)
                .is_ok(),
            "Strict mode should pass when all fields match schema"
        );
    }

    #[test]
    fn test_empty_string_values_not_null() {
        let mut serializer = make_serializer(vec![("msg", "utf8")]);

        let events = vec![create_event(vec![("msg", "")])];
        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("Empty strings should be valid");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    #[test]
    fn test_parquet_output_has_footer() {
        // Parquet files end with "PAR1" magic footer
        let mut serializer = make_serializer(vec![("msg", "utf8")]);
        let events = vec![create_event(vec![("msg", "test")])];
        let mut buffer = BytesMut::new();
        serializer.encode(events, &mut buffer).unwrap();

        let data = buffer.freeze();
        let len = data.len();
        assert!(len >= 8, "Parquet output too short");
        assert_eq!(
            &data[len - 4..],
            b"PAR1",
            "Parquet footer magic bytes missing"
        );
    }

    // ========================================================================
    // Inline nested schema: struct, list, map
    // ========================================================================

    #[test]
    fn test_inline_struct_type() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [
                {"name": "name", "type": "utf8"},
                {
                    "name": "metadata",
                    "type": "struct",
                    "fields": [
                        {"name": "source", "type": "utf8"},
                        {"name": "region", "type": "utf8"}
                    ]
                }
            ]
        }))
        .expect("Config should deserialize");

        let serializer = ParquetSerializer::new(config).expect("Struct inline schema should parse");
        assert_eq!(serializer.schema.fields().len(), 2);
        assert!(
            matches!(serializer.schema.field(1).data_type(), DataType::Struct(_)),
            "Expected Struct, got {:?}",
            serializer.schema.field(1).data_type()
        );
    }

    #[test]
    fn test_inline_list_type() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [
                {"name": "name", "type": "utf8"},
                {"name": "tags", "type": "list", "items": "utf8"}
            ]
        }))
        .expect("Config should deserialize");

        let serializer = ParquetSerializer::new(config).expect("List inline schema should parse");
        assert_eq!(serializer.schema.fields().len(), 2);
        assert!(
            matches!(serializer.schema.field(1).data_type(), DataType::List(_)),
            "Expected List, got {:?}",
            serializer.schema.field(1).data_type()
        );
    }

    #[test]
    fn test_inline_map_type() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [
                {"name": "name", "type": "utf8"},
                {
                    "name": "labels",
                    "type": "map",
                    "key_type": "utf8",
                    "value_type": "utf8"
                }
            ]
        }))
        .expect("Config should deserialize");

        let serializer = ParquetSerializer::new(config).expect("Map inline schema should parse");
        assert_eq!(serializer.schema.fields().len(), 2);
        assert!(
            matches!(serializer.schema.field(1).data_type(), DataType::Map(_, _)),
            "Expected Map, got {:?}",
            serializer.schema.field(1).data_type()
        );
    }

    #[test]
    fn test_inline_struct_missing_fields_error() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [{"name": "metadata", "type": "struct"}]
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no 'fields' defined"),
            "Expected fields error, got: {err}"
        );
    }

    #[test]
    fn test_inline_list_missing_items_error() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [{"name": "tags", "type": "list"}]
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no 'items' type defined"),
            "Expected items error, got: {err}"
        );
    }

    #[test]
    fn test_inline_map_missing_key_type_error() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [{"name": "labels", "type": "map", "value_type": "utf8"}]
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no 'key_type' defined"),
            "Expected key_type error, got: {err}"
        );
    }

    #[test]
    fn test_inline_map_missing_value_type_error() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [{"name": "labels", "type": "map", "key_type": "utf8"}]
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no 'value_type' defined"),
            "Expected value_type error, got: {err}"
        );
    }

    #[test]
    fn test_inline_struct_rejects_nested_compound_subfield() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [{
                "name": "metadata",
                "type": "struct",
                "fields": [
                    {"name": "nested", "type": "struct"}
                ]
            }]
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("must be a scalar type"),
            "Expected scalar error, got: {err}"
        );
    }

    #[test]
    fn test_inline_list_rejects_compound_items() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [{"name": "nested_lists", "type": "list", "items": "list"}]
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("must be a scalar type"),
            "Expected scalar error, got: {err}"
        );
    }

    #[test]
    fn test_inline_mixed_flat_and_nested() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [
                {"name": "message", "type": "utf8"},
                {"name": "status_code", "type": "int64"},
                {
                    "name": "metadata",
                    "type": "struct",
                    "fields": [
                        {"name": "source", "type": "utf8"},
                        {"name": "region", "type": "utf8"}
                    ]
                },
                {"name": "tags", "type": "list", "items": "utf8"},
                {
                    "name": "labels",
                    "type": "map",
                    "key_type": "utf8",
                    "value_type": "utf8"
                }
            ]
        }))
        .expect("Config should deserialize");

        let serializer = ParquetSerializer::new(config).expect("Mixed schema should parse");
        assert_eq!(serializer.schema.fields().len(), 5);
        assert_eq!(*serializer.schema.field(0).data_type(), DataType::Utf8);
        assert_eq!(*serializer.schema.field(1).data_type(), DataType::Int64);
        assert!(matches!(
            serializer.schema.field(2).data_type(),
            DataType::Struct(_)
        ));
        assert!(matches!(
            serializer.schema.field(3).data_type(),
            DataType::List(_)
        ));
        assert!(matches!(
            serializer.schema.field(4).data_type(),
            DataType::Map(_, _)
        ));
    }

    #[test]
    fn test_all_non_log_events_error() {
        use vector_core::event::{Metric, MetricKind, MetricValue};

        let mut serializer = make_serializer(vec![("msg", "utf8")]);

        let metric = Metric::new(
            "cpu.usage",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 42.0 },
        );
        let events = vec![Event::Metric(metric)];

        let mut buffer = BytesMut::new();
        let result = serializer.encode(events, &mut buffer);
        assert!(
            result.is_err(),
            "Batch of only non-log events should error"
        );
    }

    #[test]
    fn test_mixed_log_and_non_log_events() {
        use vector_core::event::{Metric, MetricKind, MetricValue};

        let mut serializer = make_serializer(vec![("msg", "utf8")]);

        let metric = Metric::new(
            "cpu.usage",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 42.0 },
        );
        let events = vec![
            create_event(vec![("msg", "hello")]),
            Event::Metric(metric),
            create_event(vec![("msg", "world")]),
        ];

        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("Mixed batch should succeed (non-log events dropped)");

        assert_parquet_magic(&buffer);
        // Only the 2 log events should be in the Parquet output
        assert_eq!(parquet_row_count(&buffer), 2);
    }

    #[test]
    fn test_inline_struct_encode_data() {
        use vector_core::event::Value;
        use vrl::value::ObjectMap;

        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [
                {"name": "name", "type": "utf8"},
                {
                    "name": "metadata",
                    "type": "struct",
                    "fields": [
                        {"name": "source", "type": "utf8"},
                        {"name": "priority", "type": "int64"}
                    ]
                }
            ]
        }))
        .expect("Config should deserialize");

        let mut serializer = ParquetSerializer::new(config).expect("Should create serializer");

        let mut meta = ObjectMap::new();
        meta.insert("source".into(), Value::Bytes("syslog".into()));
        meta.insert("priority".into(), Value::Integer(5));

        let mut log = LogEvent::default();
        log.insert("name", "test_event");
        log.insert("metadata", Value::Object(meta));

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Struct data encoding should succeed");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    #[test]
    fn test_inline_list_encode_data() {
        use vector_core::event::Value;

        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [
                {"name": "name", "type": "utf8"},
                {"name": "tags", "type": "list", "items": "utf8"}
            ]
        }))
        .expect("Config should deserialize");

        let mut serializer = ParquetSerializer::new(config).expect("Should create serializer");

        let mut log = LogEvent::default();
        log.insert("name", "test_event");
        log.insert(
            "tags",
            Value::Array(vec![
                Value::Bytes("prod".into()),
                Value::Bytes("us-east".into()),
            ]),
        );

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("List data encoding should succeed");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    #[test]
    fn test_inline_map_encode_data() {
        use vector_core::event::Value;
        use vrl::value::ObjectMap;

        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [
                {"name": "name", "type": "utf8"},
                {
                    "name": "labels",
                    "type": "map",
                    "key_type": "utf8",
                    "value_type": "utf8"
                }
            ]
        }))
        .expect("Config should deserialize");

        let mut serializer = ParquetSerializer::new(config).expect("Should create serializer");

        let mut labels = ObjectMap::new();
        labels.insert("env".into(), Value::Bytes("prod".into()));
        labels.insert("region".into(), Value::Bytes("us-east-1".into()));

        let mut log = LogEvent::default();
        log.insert("name", "test_event");
        log.insert("labels", Value::Object(labels));

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Map data encoding should succeed");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    #[test]
    fn test_strict_mode_ignores_nested_extra_fields() {
        use vector_core::event::Value;
        use vrl::value::ObjectMap;

        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema": [
                {
                    "name": "metadata",
                    "type": "struct",
                    "fields": [
                        {"name": "source", "type": "utf8"}
                    ]
                }
            ],
            "schema_mode": "strict"
        }))
        .expect("Config should deserialize");

        let mut serializer = ParquetSerializer::new(config).expect("Should create serializer");

        // Event has metadata.source (in schema) plus metadata.unknown (not in schema sub-fields)
        // Strict mode only checks top-level field names, so this should pass
        let mut meta = ObjectMap::new();
        meta.insert("source".into(), Value::Bytes("syslog".into()));
        meta.insert("unknown".into(), Value::Bytes("extra_nested".into()));

        let mut log = LogEvent::default();
        log.insert("metadata", Value::Object(meta));

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log)], &mut buffer)
            .expect("Strict mode should not reject nested extra fields");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }

    #[test]
    fn test_inline_binary_rejected_at_config_time() {
        let config = ParquetSerializerConfig {
            schema: schema_fields(vec![("payload", "binary")]),
            ..Default::default()
        };
        let result = ParquetSerializer::new(config);
        assert!(result.is_err(), "Binary should be rejected at config time");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("payload") && err.contains("binary"),
            "Error should name the field and type, got: {err}"
        );
    }

    #[test]
    fn test_avro_bytes_rejected_at_config_time() {
        // Avro "bytes" resolves to Arrow DataType::Binary, which is unsupported
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "test",
                "fields": [
                    {"name": "id", "type": "string"},
                    {"name": "blob", "type": "bytes"}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(
            result.is_err(),
            "Avro bytes should be rejected at config time"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("blob") && err.contains("Binary"),
            "Error should name the field, got: {err}"
        );
    }

    #[test]
    fn test_parquet_schema_binary_without_string_annotation_rejected() {
        // Native Parquet "binary" without (STRING) annotation resolves to Arrow Binary
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "parquet_schema": "message logs {\n  required binary name (STRING);\n  optional binary raw_data;\n}"
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(
            result.is_err(),
            "Parquet binary without STRING annotation should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("raw_data"),
            "Error should name the field, got: {err}"
        );
    }

    #[test]
    fn test_nested_binary_in_struct_rejected() {
        // Binary inside a struct should also be caught
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "test",
                "fields": [
                    {"name": "wrapper", "type": {
                        "type": "record",
                        "name": "inner",
                        "fields": [
                            {"name": "data", "type": "bytes"}
                        ]
                    }}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(
            result.is_err(),
            "Nested binary in struct should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("wrapper.data"),
            "Error should show the full dotted path, got: {err}"
        );
    }

    #[test]
    fn test_avro_nullable_union_null_value_encoding() {
        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "avro_schema": r#"{
                "type": "record",
                "name": "test",
                "fields": [
                    {"name": "id", "type": "string"},
                    {"name": "optional_count", "type": ["null", "long"]}
                ]
            }"#
        }))
        .expect("Config should deserialize");

        let mut serializer = ParquetSerializer::new(config).expect("Should create serializer");

        // First event has both fields, second has only id (optional_count is null)
        let mut log1 = LogEvent::default();
        log1.insert("id", "event_1");
        log1.insert("optional_count", vector_core::event::Value::Integer(42));

        let mut log2 = LogEvent::default();
        log2.insert("id", "event_2");
        // optional_count deliberately missing

        let mut buffer = BytesMut::new();
        serializer
            .encode(vec![Event::Log(log1), Event::Log(log2)], &mut buffer)
            .expect("Nullable union with null value should encode");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 2);
    }

    #[test]
    fn test_very_long_field_values() {
        let mut serializer = make_serializer(vec![("msg", "utf8")]);

        // 1 MB string value
        let long_value = "x".repeat(1_000_000);
        let events = vec![create_event(vec![("msg", long_value.as_str())])];

        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("Very long string value should encode");

        assert_parquet_magic(&buffer);
        assert_eq!(parquet_row_count(&buffer), 1);
    }
}
