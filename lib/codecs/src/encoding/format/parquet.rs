//! Apache Parquet format codec for batched event encoding
//!
//! Provides Apache Parquet columnar file format encoding with static schema support.
//! This encoder writes complete Parquet files with proper metadata and footers,
//! suitable for long-term storage and analytics workloads.

use arrow::datatypes::Schema;
use bytes::{Bytes, BytesMut};
use parquet::{
    arrow::ArrowWriter,
    basic::{Compression, ZstdLevel},
    file::properties::WriterProperties,
};
use snafu::Snafu;
use std::sync::Arc;
use vector_config::configurable_component;

use vector_core::event::Event;

// Reuse the Arrow encoder's record batch building logic
use super::arrow::{build_record_batch, ArrowEncodingError};
use super::schema_definition::SchemaDefinition;

/// Compression algorithm for Parquet files
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ParquetCompression {
    /// No compression
    Uncompressed,
    /// Snappy compression (fast, moderate compression ratio)
    #[default]
    Snappy,
    /// GZIP compression (slower, better compression ratio)
    Gzip,
    /// Brotli compression
    Brotli,
    /// LZ4 compression (very fast, moderate compression)
    Lz4,
    /// ZSTD compression (good balance of speed and compression)
    Zstd,
}

impl From<ParquetCompression> for Compression {
    fn from(compression: ParquetCompression) -> Self {
        match compression {
            ParquetCompression::Uncompressed => Compression::UNCOMPRESSED,
            ParquetCompression::Snappy => Compression::SNAPPY,
            ParquetCompression::Gzip => Compression::GZIP(Default::default()),
            ParquetCompression::Brotli => Compression::BROTLI(Default::default()),
            ParquetCompression::Lz4 => Compression::LZ4,
            ParquetCompression::Zstd => Compression::ZSTD(ZstdLevel::default()),
        }
    }
}

/// Configuration for Parquet serialization
#[configurable_component]
#[derive(Clone, Default)]
pub struct ParquetSerializerConfig {
    /// The Arrow schema definition to use for encoding
    ///
    /// This schema defines the structure and types of the Parquet file columns.
    /// Specified as a map of field names to data types.
    ///
    /// Supported types: utf8, int8, int16, int32, int64, uint8, uint16, uint32, uint64,
    /// float32, float64, boolean, binary, timestamp_second, timestamp_millisecond,
    /// timestamp_microsecond, timestamp_nanosecond, date32, date64, and more.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "schema_example()"))]
    pub schema: Option<SchemaDefinition>,

    /// Compression algorithm to use for Parquet columns
    ///
    /// Compression is applied to all columns in the Parquet file.
    /// Snappy provides a good balance of speed and compression ratio.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "snappy"))]
    #[configurable(metadata(docs::examples = "gzip"))]
    #[configurable(metadata(docs::examples = "zstd"))]
    pub compression: ParquetCompression,

    /// Number of rows per row group
    ///
    /// Row groups are Parquet's unit of parallelization. Larger row groups
    /// can improve compression but increase memory usage during encoding.
    ///
    /// Since each batch becomes a separate Parquet file, this value
    /// should be <= the batch max_events setting. Row groups cannot span multiple files.
    /// If not specified, defaults to the batch size.
    #[serde(default)]
    #[configurable(metadata(docs::examples = 100000))]
    #[configurable(metadata(docs::examples = 1000000))]
    pub row_group_size: Option<usize>,

    /// Allow null values for non-nullable fields in the schema.
    ///
    /// When enabled, missing or incompatible values will be encoded as null even for fields
    /// marked as non-nullable in the Arrow schema. This is useful when working with downstream
    /// systems that can handle null values through defaults, computed columns, or other mechanisms.
    ///
    /// When disabled (default), missing values for non-nullable fields will cause encoding errors,
    /// ensuring all required data is present before writing to Parquet.
    #[serde(default)]
    #[configurable(metadata(docs::examples = true))]
    pub allow_nullable_fields: bool,
}

fn schema_example() -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    map.insert("id".to_string(), "int64".to_string());
    map.insert("name".to_string(), "utf8".to_string());
    map.insert("timestamp".to_string(), "timestamp_microsecond".to_string());
    map
}

impl std::fmt::Debug for ParquetSerializerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParquetSerializerConfig")
            .field("schema", &self.schema.is_some())
            .field("compression", &self.compression)
            .field("row_group_size", &self.row_group_size)
            .field("allow_nullable_fields", &self.allow_nullable_fields)
            .finish()
    }
}

impl ParquetSerializerConfig {
    /// Create a new ParquetSerializerConfig with a schema definition
    pub fn new(schema: SchemaDefinition) -> Self {
        Self {
            schema: Some(schema),
            compression: ParquetCompression::default(),
            row_group_size: None,
            allow_nullable_fields: false,
        }
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

/// Parquet batch serializer that holds the schema and writer configuration
#[derive(Clone, Debug)]
pub struct ParquetSerializer {
    schema: Arc<Schema>,
    writer_properties: WriterProperties,
}

impl ParquetSerializer {
    /// Create a new ParquetSerializer with the given configuration
    pub fn new(config: ParquetSerializerConfig) -> Result<Self, vector_common::Error> {
        let schema_def = config.schema.ok_or_else(|| {
            vector_common::Error::from(
                "Parquet serializer requires a schema. Specify 'schema' in the configuration."
            )
        })?;

        // Convert SchemaDefinition to Arrow Schema
        let mut schema = schema_def
            .to_arrow_schema()
            .map_err(|e| vector_common::Error::from(e.to_string()))?;

        // If allow_nullable_fields is enabled, transform the schema once here
        // instead of on every batch encoding
        if config.allow_nullable_fields {
            schema = Arc::new(Schema::new_with_metadata(
                schema
                    .fields()
                    .iter()
                    .map(|f| Arc::new(super::arrow::make_field_nullable(f)))
                    .collect::<Vec<_>>(),
                schema.metadata().clone(),
            ));
        }

        // Build writer properties
        let mut props_builder = WriterProperties::builder()
            .set_compression(config.compression.into());

        if let Some(row_group_size) = config.row_group_size {
            props_builder = props_builder.set_max_row_group_size(row_group_size);
        }

        let writer_properties = props_builder.build();

        Ok(Self {
            schema,
            writer_properties,
        })
    }
}

impl tokio_util::codec::Encoder<Vec<Event>> for ParquetSerializer {
    type Error = ParquetEncodingError;

    fn encode(&mut self, events: Vec<Event>, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        if events.is_empty() {
            return Err(ParquetEncodingError::NoEvents);
        }

        let bytes = encode_events_to_parquet(&events, Arc::clone(&self.schema), &self.writer_properties)?;

        buffer.extend_from_slice(&bytes);
        Ok(())
    }
}

/// Errors that can occur during Parquet encoding
#[derive(Debug, Snafu)]
pub enum ParquetEncodingError {
    /// Failed to build Arrow record batch
    #[snafu(display("Failed to build Arrow record batch: {}", source))]
    RecordBatchCreation {
        /// The underlying Arrow encoding error
        source: ArrowEncodingError,
    },

    /// Failed to write Parquet data
    #[snafu(display("Failed to write Parquet data: {}", source))]
    ParquetWrite {
        /// The underlying Parquet error
        source: parquet::errors::ParquetError,
    },

    /// No events provided for encoding
    #[snafu(display("No events provided for encoding"))]
    NoEvents,

    /// Schema must be provided before encoding
    #[snafu(display("Schema must be provided before encoding"))]
    NoSchemaProvided,

    /// IO error during encoding
    #[snafu(display("IO error: {}", source))]
    Io {
        /// The underlying IO error
        source: std::io::Error,
    },
}

impl From<std::io::Error> for ParquetEncodingError {
    fn from(error: std::io::Error) -> Self {
        Self::Io { source: error }
    }
}

impl From<ArrowEncodingError> for ParquetEncodingError {
    fn from(error: ArrowEncodingError) -> Self {
        Self::RecordBatchCreation { source: error }
    }
}

impl From<parquet::errors::ParquetError> for ParquetEncodingError {
    fn from(error: parquet::errors::ParquetError) -> Self {
        Self::ParquetWrite { source: error }
    }
}

/// Encodes a batch of events into Parquet format
pub fn encode_events_to_parquet(
    events: &[Event],
    schema: Arc<Schema>,
    writer_properties: &WriterProperties,
) -> Result<Bytes, ParquetEncodingError> {
    if events.is_empty() {
        return Err(ParquetEncodingError::NoEvents);
    }

    // Build Arrow RecordBatch from events (reuses Arrow encoder logic)
    let record_batch = build_record_batch(schema, events)?;

    // Write RecordBatch to Parquet format in memory
    let mut buffer = Vec::new();
    {
        let mut writer = ArrowWriter::try_new(
            &mut buffer,
            record_batch.schema(),
            Some(writer_properties.clone()),
        )?;

        writer.write(&record_batch)?;
        writer.close()?;
    }

    Ok(Bytes::from(buffer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::{
        array::{
            Array, BinaryArray, BooleanArray, Float64Array, Int64Array, StringArray,
            TimestampMicrosecondArray,
        },
        datatypes::{DataType, Field, TimeUnit},
    };
    use bytes::Bytes;
    use chrono::Utc;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use vector_core::event::LogEvent;

    #[test]
    fn test_encode_all_types() {
        let mut log = LogEvent::default();
        log.insert("string_field", "test");
        log.insert("int64_field", 42);
        log.insert("float64_field", 3.15);
        log.insert("bool_field", true);
        log.insert("bytes_field", bytes::Bytes::from("binary"));
        log.insert("timestamp_field", Utc::now());

        let events = vec![Event::Log(log)];

        let schema = Arc::new(Schema::new(vec![
            Field::new("string_field", DataType::Utf8, true),
            Field::new("int64_field", DataType::Int64, true),
            Field::new("float64_field", DataType::Float64, true),
            Field::new("bool_field", DataType::Boolean, true),
            Field::new("bytes_field", DataType::Binary, true),
            Field::new(
                "timestamp_field",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                true,
            ),
        ]));

        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build();

        let result = encode_events_to_parquet(&events, Arc::clone(&schema), &props);
        assert!(result.is_ok());

        let bytes = result.unwrap();

        // Verify it's valid Parquet by reading it back
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes)
            .unwrap()
            .build()
            .unwrap();

        let batches: Vec<_> = reader.collect::<Result<_, _>>().unwrap();
        assert_eq!(batches.len(), 1);

        let batch = &batches[0];
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 6);

        // Verify string field
        assert_eq!(
            batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0),
            "test"
        );

        // Verify int64 field
        assert_eq!(
            batch
                .column(1)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .value(0),
            42
        );

        // Verify float64 field
        assert!(
            (batch
                .column(2)
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .value(0)
                - 3.15)
                .abs()
                < 0.001
        );

        // Verify boolean field
        assert!(
            batch
                .column(3)
                .as_any()
                .downcast_ref::<BooleanArray>()
                .unwrap()
                .value(0)
        );

        // Verify binary field
        assert_eq!(
            batch
                .column(4)
                .as_any()
                .downcast_ref::<BinaryArray>()
                .unwrap()
                .value(0),
            b"binary"
        );

        // Verify timestamp field
        assert!(
            !batch
                .column(5)
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .unwrap()
                .is_null(0)
        );
    }

    #[test]
    fn test_encode_null_values() {
        let mut log1 = LogEvent::default();
        log1.insert("field_a", 1);
        // field_b is missing

        let mut log2 = LogEvent::default();
        log2.insert("field_b", 2);
        // field_a is missing

        let events = vec![Event::Log(log1), Event::Log(log2)];

        let schema = Arc::new(Schema::new(vec![
            Field::new("field_a", DataType::Int64, true),
            Field::new("field_b", DataType::Int64, true),
        ]));

        let props = WriterProperties::builder().build();

        let result = encode_events_to_parquet(&events, Arc::clone(&schema), &props);
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes)
            .unwrap()
            .build()
            .unwrap();

        let batches: Vec<_> = reader.collect::<Result<_, _>>().unwrap();
        let batch = &batches[0];

        assert_eq!(batch.num_rows(), 2);

        let field_a = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(field_a.value(0), 1);
        assert!(field_a.is_null(1));

        let field_b = batch
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert!(field_b.is_null(0));
        assert_eq!(field_b.value(1), 2);
    }

    #[test]
    fn test_encode_empty_events() {
        let events: Vec<Event> = vec![];
        let schema = Arc::new(Schema::new(vec![Field::new(
            "field",
            DataType::Int64,
            true,
        )]));
        let props = WriterProperties::builder().build();
        let result = encode_events_to_parquet(&events, schema, &props);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParquetEncodingError::NoEvents));
    }

    #[test]
    fn test_parquet_compression_types() {
        let mut log = LogEvent::default();
        log.insert("message", "test message");

        let events = vec![Event::Log(log)];
        let schema = Arc::new(Schema::new(vec![Field::new(
            "message",
            DataType::Utf8,
            true,
        )]));

        // Test different compression algorithms
        let compressions = vec![
            ParquetCompression::Uncompressed,
            ParquetCompression::Snappy,
            ParquetCompression::Gzip,
            ParquetCompression::Zstd,
        ];

        for compression in compressions {
            let props = WriterProperties::builder()
                .set_compression(compression.into())
                .build();

            let result = encode_events_to_parquet(&events, Arc::clone(&schema), &props);
            assert!(result.is_ok(), "Failed with compression: {:?}", compression);

            // Verify we can read it back
            let bytes = result.unwrap();
            let reader = ParquetRecordBatchReaderBuilder::try_new(bytes)
                .unwrap()
                .build()
                .unwrap();

            let batches: Vec<_> = reader.collect::<Result<_, _>>().unwrap();
            assert_eq!(batches[0].num_rows(), 1);
        }
    }

    #[test]
    fn test_parquet_serializer_config() {
        use std::collections::BTreeMap;

        let mut schema_map = BTreeMap::new();
        schema_map.insert("field".to_string(), "int64".to_string());

        let config = ParquetSerializerConfig {
            schema: Some(SchemaDefinition::Simple(schema_map)),
            compression: ParquetCompression::Zstd,
            row_group_size: Some(1000),
            allow_nullable_fields: false,
        };

        let serializer = ParquetSerializer::new(config);
        assert!(serializer.is_ok());
    }

    #[test]
    fn test_parquet_serializer_no_schema_fails() {
        let config = ParquetSerializerConfig {
            schema: None,
            compression: ParquetCompression::default(),
            row_group_size: None,
            allow_nullable_fields: false,
        };

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_encoder_trait_implementation() {
        use std::collections::BTreeMap;
        use tokio_util::codec::Encoder;

        let mut schema_map = BTreeMap::new();
        schema_map.insert("id".to_string(), "int64".to_string());
        schema_map.insert("name".to_string(), "utf8".to_string());

        let config = ParquetSerializerConfig::new(SchemaDefinition::Simple(schema_map));
        let mut serializer = ParquetSerializer::new(config).unwrap();

        let mut log = LogEvent::default();
        log.insert("id", 1);
        log.insert("name", "test");

        let events = vec![Event::Log(log)];
        let mut buffer = BytesMut::new();

        let result = serializer.encode(events, &mut buffer);
        assert!(result.is_ok());
        assert!(!buffer.is_empty());

        // Verify the buffer contains valid Parquet data
        let bytes = Bytes::copy_from_slice(&buffer);
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes);
        assert!(reader.is_ok());
    }

    #[test]
    fn test_large_batch_encoding() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, true),
            Field::new("value", DataType::Float64, true),
        ]));

        // Create 10,000 events
        let events: Vec<Event> = (0..10000)
            .map(|i| {
                let mut log = LogEvent::default();
                log.insert("id", i);
                log.insert("value", i as f64 * 1.5);
                Event::Log(log)
            })
            .collect();

        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .set_max_row_group_size(5000) // 2 row groups
            .build();

        let result = encode_events_to_parquet(&events, Arc::clone(&schema), &props);
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes)
            .unwrap()
            .build()
            .unwrap();

        let batches: Vec<_> = reader.collect::<Result<_, _>>().unwrap();
        let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total_rows, 10000);
    }

    #[test]
    fn test_allow_nullable_fields_config() {
        use std::collections::BTreeMap;
        use tokio_util::codec::Encoder;

        let mut schema_map = BTreeMap::new();
        schema_map.insert("required_field".to_string(), "int64".to_string());

        let mut log1 = LogEvent::default();
        log1.insert("required_field", 42);

        let log2 = LogEvent::default();
        // log2 is missing required_field

        let events = vec![Event::Log(log1), Event::Log(log2)];

        // Note: SchemaDefinition creates nullable fields by default
        // This test verifies that the allow_nullable_fields flag works
        let mut config = ParquetSerializerConfig::new(SchemaDefinition::Simple(schema_map));
        config.allow_nullable_fields = true;

        let mut serializer = ParquetSerializer::new(config).unwrap();
        let mut buffer = BytesMut::new();
        let result = serializer.encode(events.clone(), &mut buffer);
        assert!(result.is_ok());

        // Verify the data
        let bytes = Bytes::copy_from_slice(&buffer);
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes)
            .unwrap()
            .build()
            .unwrap();

        let batches: Vec<_> = reader.collect::<Result<_, _>>().unwrap();
        let batch = &batches[0];

        let array = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();

        assert_eq!(array.value(0), 42);
        assert!(array.is_null(1));
    }
}
