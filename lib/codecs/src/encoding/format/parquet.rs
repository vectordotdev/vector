//! Parquet batch format codec for batched event encoding
//!
//! Provides Apache Parquet format encoding with static schema support.
//! This reuses the Arrow record batch building logic from the Arrow IPC codec,
//! then writes the batch as a complete Parquet file using `ArrowWriter`.

use std::sync::Arc;

use arrow::datatypes::{Field, Schema, SchemaRef};
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

/// Configuration for the Parquet serializer.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct ParquetSerializerConfig {
    /// The Arrow schema for Parquet encoding.
    #[serde(skip)]
    #[configurable(derived)]
    pub schema: Option<Schema>,

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
            schema: None,
            compression: ParquetCompression::default(),
            schema_mode: SchemaMode::default(),
        }
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
            .schema
            .ok_or("Parquet serializer requires a schema")?;

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
    use arrow::datatypes::DataType;
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

    /// Helper to build a ParquetSerializer with a given schema and defaults
    fn make_serializer(schema: Schema) -> ParquetSerializer {
        ParquetSerializer::new(ParquetSerializerConfig {
            schema: Some(schema),
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

        let schema = Schema::new(vec![
            Field::new("name", DataType::Utf8, true),
            Field::new("age", DataType::Int64, true),
        ]);

        let mut serializer = make_serializer(schema);

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
        let schema = Schema::new(vec![
            Field::new("name", DataType::Utf8, true),
            Field::new("age", DataType::Int64, true),
        ]);

        let mut serializer = make_serializer(schema);

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
        let schema = Schema::new(vec![Field::new("name", DataType::Utf8, true)]);

        let mut serializer = make_serializer(schema);

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
        let schema = Schema::new(vec![Field::new("name", DataType::Utf8, true)]);

        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema: Some(schema),
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
        let schema = Schema::new(vec![Field::new("msg", DataType::Utf8, true)]);
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
                schema: Some(schema.clone()),
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
        let schema = Schema::new(vec![Field::new("msg", DataType::Utf8, true)]);
        let mut serializer = make_serializer(schema);

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
}
