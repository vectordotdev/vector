//! Parquet batch format codec for batched event encoding
//!
//! Provides Apache Parquet format encoding with static schema support.
//! This reuses the Arrow record batch building logic from the Arrow IPC codec,
//! then writes the batch as a complete Parquet file using `ArrowWriter`.

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use bytes::{BufMut, BytesMut};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression as ParquetCodecCompression;
use parquet::file::properties::WriterProperties;
use vector_config::configurable_component;
use vector_core::event::Event;

use super::arrow::{build_record_batch, ArrowEncodingError};

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
}

impl ParquetFieldType {
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
                DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into()))
            }
            Self::TimestampMicrosecond => {
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()))
            }
            Self::TimestampNanosecond => {
                DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into()))
            }
            Self::Date32 => DataType::Date32,
        }
    }
}

/// A field definition for the Parquet schema.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct ParquetSchemaField {
    /// The name of the field.
    pub name: String,

    /// The data type of the field.
    #[serde(rename = "type")]
    pub data_type: ParquetFieldType,
}

/// Configuration for the Parquet serializer.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct ParquetSerializerConfig {
    /// The schema definition for Parquet encoding.
    ///
    /// Each entry defines a column with a name and data type.
    /// All fields are made nullable automatically.
    #[serde(default)]
    #[configurable(derived)]
    pub schema: Vec<ParquetSchemaField>,

    /// Compression codec for Parquet columns.
    #[serde(default)]
    #[configurable(derived)]
    pub compression: ParquetCompression,

    /// Schema handling mode.
    #[serde(default)]
    #[configurable(derived)]
    pub schema_mode: SchemaMode,
}

impl Default for ParquetSerializerConfig {
    fn default() -> Self {
        Self {
            schema: Vec::new(),
            compression: ParquetCompression::default(),
            schema_mode: SchemaMode::default(),
        }
    }
}

impl ParquetSerializerConfig {
    /// Convert the user-facing schema config to an Arrow Schema.
    fn to_arrow_schema(&self) -> Option<Schema> {
        if self.schema.is_empty() {
            return None;
        }
        let fields: Vec<Field> = self
            .schema
            .iter()
            .map(|f| Field::new(&f.name, f.data_type.to_arrow_data_type(), true))
            .collect();
        Some(Schema::new(fields))
    }
}

impl ParquetSerializerConfig {
    /// The data type of events that are accepted by `ParquetSerializer`.
    pub fn input_type(&self) -> vector_core::config::DataType {
        vector_core::config::DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> vector_core::schema::Requirement {
        vector_core::schema::Requirement::empty()
    }
}

/// Parquet batch serializer.
#[derive(Clone, Debug)]
pub struct ParquetSerializer {
    schema: SchemaRef,
    writer_props: WriterProperties,
    schema_mode: SchemaMode,
}

impl ParquetSerializer {
    /// Create a new `ParquetSerializer` from the given configuration.
    pub fn new(
        config: ParquetSerializerConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let schema = config
            .to_arrow_schema()
            .ok_or("Parquet serializer requires a schema with at least one field")?;

        // Make all fields nullable for compatibility
        let nullable_schema = Schema::new(
            schema
                .fields()
                .iter()
                .map(|f| Arc::new(Field::new(f.name(), f.data_type().clone(), true)))
                .collect::<Vec<_>>(),
        );

        let writer_props = WriterProperties::builder()
            .set_compression(config.compression.to_parquet_compression())
            .build();

        Ok(Self {
            schema: SchemaRef::new(nullable_schema),
            writer_props,
            schema_mode: config.schema_mode,
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

        // In strict mode, check for extra fields not in the schema
        if self.schema_mode == SchemaMode::Strict {
            for event in &events {
                if let Some(log) = event.maybe_as_log() {
                    for (key, _) in log.all_event_fields().expect("log event should have fields")
                    {
                        // Strip the leading '.' that Vector adds to field paths
                        let field_name = key.strip_prefix('.').unwrap_or(&key);
                        if self.schema.field_with_name(field_name).is_err() {
                            return Err(Box::new(ArrowEncodingError::SchemaFetchError {
                                message: format!(
                                    "Strict schema mode: event contains field '{}' not in schema",
                                    field_name
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

        // Write as a complete Parquet file to an in-memory buffer
        let mut buf = Vec::new();
        let mut writer =
            ArrowWriter::try_new(&mut buf, self.schema.clone(), Some(self.writer_props.clone()))?;
        writer.write(&record_batch)?;
        writer.close()?;

        buffer.put_slice(&buf);
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

    /// Helper to create schema fields from (name, type) pairs
    fn schema_fields(fields: Vec<(&str, ParquetFieldType)>) -> Vec<ParquetSchemaField> {
        fields
            .into_iter()
            .map(|(name, data_type)| ParquetSchemaField {
                name: name.to_string(),
                data_type,
            })
            .collect()
    }

    /// Helper to build a ParquetSerializer with given fields and defaults
    fn make_serializer(fields: Vec<(&str, ParquetFieldType)>) -> ParquetSerializer {
        ParquetSerializer::new(ParquetSerializerConfig {
            schema: schema_fields(fields),
            compression: ParquetCompression::default(),
            schema_mode: SchemaMode::default(),
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

        let mut serializer = make_serializer(vec![
            ("name", ParquetFieldType::Utf8),
            ("age", ParquetFieldType::Int64),
        ]);

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
        let mut serializer = make_serializer(vec![
            ("name", ParquetFieldType::Utf8),
            ("age", ParquetFieldType::Int64),
        ]);

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
        let mut serializer = make_serializer(vec![("name", ParquetFieldType::Utf8)]);

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
            schema: schema_fields(vec![("name", ParquetFieldType::Utf8)]),
            compression: ParquetCompression::default(),
            schema_mode: SchemaMode::Strict,
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
        let fields = schema_fields(vec![("msg", ParquetFieldType::Utf8)]);
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
                schema: fields.clone(),
                compression,
                schema_mode: SchemaMode::default(),
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
        let mut serializer = make_serializer(vec![("msg", ParquetFieldType::Utf8)]);

        let events: Vec<Event> = vec![];
        let mut buffer = BytesMut::new();

        serializer
            .encode(events, &mut buffer)
            .expect("Empty events should succeed");

        assert!(
            buffer.is_empty(),
            "Buffer should be empty for empty events"
        );
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
        assert!(
            result.is_err(),
            "Should fail when schema has no fields"
        );
    }
}
