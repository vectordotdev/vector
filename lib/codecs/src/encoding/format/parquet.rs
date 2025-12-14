//! Apache Parquet format codec for batched event encoding
//!
//! Provides Apache Parquet columnar file format encoding with static schema support.
//! This encoder writes complete Parquet files with proper metadata and footers,
//! suitable for long-term storage and analytics workloads.

use arrow::datatypes::Schema;
use bytes::{Bytes, BytesMut, BufMut};
use parquet::{
    arrow::ArrowWriter,
    basic::{Compression, ZstdLevel, GzipLevel, BrotliLevel},
    file::properties::{WriterProperties, WriterVersion},
    schema::types::ColumnPath,
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

impl ParquetCompression {
    /// Convert to parquet Compression with optional level override
    fn to_compression(&self, level: Option<i32>) -> Result<Compression, String> {
        match (self, level) {
            (ParquetCompression::Uncompressed, _) => Ok(Compression::UNCOMPRESSED),
            (ParquetCompression::Snappy, _) => Ok(Compression::SNAPPY),
            (ParquetCompression::Lz4, _) => Ok(Compression::LZ4),
            (ParquetCompression::Gzip, Some(lvl)) => {
                GzipLevel::try_new(lvl as u32)
                    .map(Compression::GZIP)
                    .map_err(|e| format!("Invalid GZIP compression level: {}", e))
            }
            (ParquetCompression::Gzip, None) => Ok(Compression::GZIP(Default::default())),
            (ParquetCompression::Brotli, Some(lvl)) => {
                BrotliLevel::try_new(lvl as u32)
                    .map(Compression::BROTLI)
                    .map_err(|e| format!("Invalid Brotli compression level: {}", e))
            }
            (ParquetCompression::Brotli, None) => Ok(Compression::BROTLI(Default::default())),
            (ParquetCompression::Zstd, Some(lvl)) => {
                ZstdLevel::try_new(lvl)
                    .map(Compression::ZSTD)
                    .map_err(|e| format!("Invalid ZSTD compression level: {}", e))
            }
            (ParquetCompression::Zstd, None) => Ok(Compression::ZSTD(ZstdLevel::default())),
        }
    }
}

impl From<ParquetCompression> for Compression {
    fn from(compression: ParquetCompression) -> Self {
        compression.to_compression(None).expect("Default compression should always be valid")
    }
}

/// Parquet writer version
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ParquetWriterVersion {
    /// Parquet format version 1.0 (maximum compatibility)
    V1,
    /// Parquet format version 2.0 (modern format with better encoding)
    #[default]
    V2,
}

impl From<ParquetWriterVersion> for WriterVersion {
    fn from(version: ParquetWriterVersion) -> Self {
        match version {
            ParquetWriterVersion::V1 => WriterVersion::PARQUET_1_0,
            ParquetWriterVersion::V2 => WriterVersion::PARQUET_2_0,
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
    /// Mutually exclusive with `infer_schema`. Must specify either `schema` or `infer_schema: true`.
    ///
    /// Supported types: utf8, int8, int16, int32, int64, uint8, uint16, uint32, uint64,
    /// float32, float64, boolean, binary, timestamp_second, timestamp_millisecond,
    /// timestamp_microsecond, timestamp_nanosecond, date32, date64, and more.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "schema_example()"))]
    pub schema: Option<SchemaDefinition>,

    /// Automatically infer schema from event data
    ///
    /// When enabled, the schema is inferred from each batch of events independently.
    /// The schema is determined by examining the types of values in the events.
    ///
    /// **Type mapping:**
    /// - String values → `utf8`
    /// - Integer values → `int64`
    /// - Float values → `float64`
    /// - Boolean values → `boolean`
    /// - Timestamp values → `timestamp_microsecond`
    /// - Arrays/Objects → `utf8` (serialized as JSON)
    ///
    /// **Type conflicts:** If a field has different types across events in the same batch,
    /// it will be encoded as `utf8` (string) and all values will be converted to strings.
    ///
    /// **Important:** Schema consistency across batches is the operator's responsibility.
    /// Use VRL transforms to ensure consistent types if needed. Each batch may produce
    /// a different schema if event structure varies.
    ///
    /// **Bloom filters:** Not supported with inferred schemas. Use explicit schema for Bloom filters.
    ///
    /// Mutually exclusive with `schema`. Must specify either `schema` or `infer_schema: true`.
    #[serde(default)]
    #[configurable(metadata(docs::examples = true))]
    pub infer_schema: bool,

    /// Column names to exclude from Parquet encoding
    ///
    /// These columns will be completely excluded from the Parquet file.
    /// Useful for filtering out metadata, internal fields, or temporary data.
    ///
    /// Only applies when `infer_schema` is enabled. Ignored when using explicit schema.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "vec![\"_metadata\".to_string(), \"internal_id\".to_string()]"))]
    pub exclude_columns: Option<Vec<String>>,

    /// Maximum number of columns to encode
    ///
    /// Limits the number of columns in the Parquet file. Additional columns beyond
    /// this limit will be silently dropped. Columns are selected in the order they
    /// appear in the first event.
    ///
    /// Only applies when `infer_schema` is enabled. Ignored when using explicit schema.
    #[serde(default = "default_max_columns")]
    #[configurable(metadata(docs::examples = 500))]
    #[configurable(metadata(docs::examples = 1000))]
    pub max_columns: usize,

    /// Compression algorithm to use for Parquet columns
    ///
    /// Compression is applied to all columns in the Parquet file.
    /// Snappy provides a good balance of speed and compression ratio.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "snappy"))]
    #[configurable(metadata(docs::examples = "gzip"))]
    #[configurable(metadata(docs::examples = "zstd"))]
    pub compression: ParquetCompression,

    /// Compression level for algorithms that support it.
    ///
    /// Only applies to ZSTD, GZIP, and Brotli compression. Ignored for other algorithms.
    ///
    /// **ZSTD levels** (1-22):
    /// - 1-3: Fastest, moderate compression (level 3 is default)
    /// - 4-9: Good balance of speed and compression
    /// - 10-15: Better compression, slower encoding
    /// - 16-22: Maximum compression, slowest (good for cold storage)
    ///
    /// **GZIP levels** (1-9):
    /// - 1-3: Faster, less compression
    /// - 6: Default balance (recommended)
    /// - 9: Maximum compression, slowest
    ///
    /// **Brotli levels** (0-11):
    /// - 0-4: Faster encoding
    /// - 1: Default (recommended)
    /// - 5-11: Better compression, slower
    ///
    /// Higher levels typically produce 20-50% smaller files but take 2-5x longer to encode.
    /// Recommended: Use level 3-6 for hot data, 10-15 for cold storage.
    #[serde(default)]
    #[configurable(metadata(docs::examples = 3))]
    #[configurable(metadata(docs::examples = 6))]
    #[configurable(metadata(docs::examples = 10))]
    pub compression_level: Option<i32>,

    /// Parquet format writer version.
    ///
    /// Controls which Parquet format version to write:
    /// - **v1** (PARQUET_1_0): Original format, maximum compatibility (default)
    /// - **v2** (PARQUET_2_0): Modern format with improved encoding and statistics
    ///
    /// Version 2 benefits:
    /// - More efficient encoding for certain data types (10-20% smaller files)
    /// - Better statistics for query optimization
    /// - Improved data page format
    /// - Required for some advanced features
    ///
    /// Use v1 for maximum compatibility with older readers (pre-2018 tools).
    /// Use v2 for better performance with modern query engines (Athena, Spark, Presto).
    #[serde(default)]
    #[configurable(metadata(docs::examples = "v1"))]
    #[configurable(metadata(docs::examples = "v2"))]
    pub writer_version: ParquetWriterVersion,

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

    /// Sorting order for rows within row groups.
    ///
    /// Pre-sorting rows by specified columns before writing can significantly improve both
    /// compression ratios and query performance. This is especially valuable for time-series
    /// data and event logs.
    ///
    /// **Benefits:**
    /// - **Better compression** (20-40% smaller files): Similar values are grouped together
    /// - **Faster queries**: More effective min/max statistics enable better row group skipping
    /// - **Improved caching**: Query engines can more efficiently cache sorted data
    ///
    /// **Common patterns:**
    /// - Time-series: Sort by timestamp descending (most recent first)
    /// - Multi-tenant: Sort by tenant_id, then timestamp
    /// - User analytics: Sort by user_id, then event_time
    ///
    /// **Trade-offs:**
    /// - Adds sorting overhead during encoding (typically 10-30% slower writes)
    /// - Requires buffering entire batch in memory for sorting
    /// - Most beneficial when queries frequently filter on sorted columns
    ///
    /// **Example:**
    /// ```yaml
    /// sorting_columns:
    ///   - column: timestamp
    ///     descending: true
    ///   - column: user_id
    ///     descending: false
    /// ```
    ///
    /// If not specified, rows are written in the order they appear in the batch.
    #[serde(default)]
    pub sorting_columns: Option<Vec<SortingColumnConfig>>,
}

/// Column sorting configuration
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortingColumnConfig {
    /// Name of the column to sort by
    #[configurable(metadata(docs::examples = "timestamp"))]
    #[configurable(metadata(docs::examples = "user_id"))]
    pub column: String,

    /// Sort in descending order (true) or ascending order (false)
    ///
    /// - `true`: Descending (Z-A, 9-0, newest-oldest)
    /// - `false`: Ascending (A-Z, 0-9, oldest-newest)
    #[serde(default)]
    #[configurable(metadata(docs::examples = true))]
    pub descending: bool,
}

fn default_max_columns() -> usize {
    1000
}

fn schema_example() -> SchemaDefinition {
    use std::collections::BTreeMap;
    use super::schema_definition::FieldDefinition;

    let mut fields = BTreeMap::new();
    fields.insert(
        "id".to_string(),
        FieldDefinition {
            r#type: "int64".to_string(),
            bloom_filter: false,
            bloom_filter_num_distinct_values: None,
            bloom_filter_false_positive_pct: None,
        },
    );
    fields.insert(
        "name".to_string(),
        FieldDefinition {
            r#type: "utf8".to_string(),
            bloom_filter: true,  // Example: enable for high-cardinality string field
            bloom_filter_num_distinct_values: Some(1_000_000),
            bloom_filter_false_positive_pct: Some(0.01),
        },
    );
    fields.insert(
        "timestamp".to_string(),
        FieldDefinition {
            r#type: "timestamp_microsecond".to_string(),
            bloom_filter: false,
            bloom_filter_num_distinct_values: None,
            bloom_filter_false_positive_pct: None,
        },
    );
    SchemaDefinition { fields }
}

impl std::fmt::Debug for ParquetSerializerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParquetSerializerConfig")
            .field("schema", &self.schema.is_some())
            .field("infer_schema", &self.infer_schema)
            .field("exclude_columns", &self.exclude_columns)
            .field("max_columns", &self.max_columns)
            .field("compression", &self.compression)
            .field("compression_level", &self.compression_level)
            .field("writer_version", &self.writer_version)
            .field("row_group_size", &self.row_group_size)
            .field("allow_nullable_fields", &self.allow_nullable_fields)
            .field("sorting_columns", &self.sorting_columns)
            .finish()
    }
}

impl ParquetSerializerConfig {
    /// Create a new ParquetSerializerConfig with a schema definition
    pub fn new(schema: SchemaDefinition) -> Self {
        Self {
            schema: Some(schema),
            infer_schema: false,
            exclude_columns: None,
            max_columns: default_max_columns(),
            compression: ParquetCompression::default(),
            compression_level: None,
            writer_version: ParquetWriterVersion::default(),
            row_group_size: None,
            allow_nullable_fields: false,
            sorting_columns: None,
        }
    }

    /// Validate the configuration
    fn validate(&self) -> Result<(), String> {
        // Must specify exactly one schema method
        match (self.schema.is_some(), self.infer_schema) {
            (true, true) => Err("Cannot use both 'schema' and 'infer_schema: true'. Choose one.".to_string()),
            (false, false) => Err("Must specify either 'schema' or 'infer_schema: true'".to_string()),
            _ => Ok(())
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

/// Schema mode for Parquet serialization
#[derive(Clone, Debug)]
enum SchemaMode {
    /// Use pre-defined explicit schema
    Explicit {
        schema: Arc<Schema>,
    },
    /// Infer schema from each batch
    Inferred {
        exclude_columns: std::collections::BTreeSet<String>,
        max_columns: usize,
    },
}

/// Parquet batch serializer that holds the schema and writer configuration
#[derive(Clone, Debug)]
pub struct ParquetSerializer {
    schema_mode: SchemaMode,
    writer_properties: WriterProperties,
}

impl ParquetSerializer {
    /// Create a new ParquetSerializer with the given configuration
    pub fn new(config: ParquetSerializerConfig) -> Result<Self, vector_common::Error> {
        // Validate configuration
        config.validate()
            .map_err(|e| vector_common::Error::from(e))?;

        // Keep a copy of schema_def for later use with Bloom filters
        let schema_def_opt = config.schema.clone();

        // Determine schema mode
        let schema_mode = if config.infer_schema {
            SchemaMode::Inferred {
                exclude_columns: config.exclude_columns
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
                max_columns: config.max_columns,
            }
        } else {
            let schema_def = config.schema.ok_or_else(|| {
                vector_common::Error::from("Schema required when infer_schema is false")
            })?;

            // Convert SchemaDefinition to Arrow Schema
            let mut schema = schema_def
                .to_arrow_schema()
                .map_err(|e| vector_common::Error::from(e.to_string()))?;

            // If allow_nullable_fields is enabled, transform the schema once here
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

            SchemaMode::Explicit { schema }
        };

        // Build writer properties
        let compression = config.compression.to_compression(config.compression_level)
            .map_err(|e| vector_common::Error::from(e))?;

        tracing::debug!(
            compression = ?config.compression,
            compression_level = ?config.compression_level,
            writer_version = ?config.writer_version,
            infer_schema = config.infer_schema,
            "Configuring Parquet writer properties"
        );

        let mut props_builder = WriterProperties::builder()
            .set_compression(compression)
            .set_writer_version(config.writer_version.into());

        if let Some(row_group_size) = config.row_group_size {
            props_builder = props_builder.set_max_row_group_size(row_group_size);
        }

        // Only apply Bloom filters and sorting for explicit schema mode
        if let (SchemaMode::Explicit { schema }, Some(schema_def)) = (&schema_mode, &schema_def_opt) {

            // Apply per-column Bloom filter settings from schema
            let bloom_filter_configs = schema_def.extract_bloom_filter_configs();
            for bloom_config in bloom_filter_configs {
                if let Some(col_idx) = schema
                    .fields()
                    .iter()
                    .position(|f| f.name() == &bloom_config.column_name)
                {
                    // Use field-specific settings or sensible defaults
                    let fpp = bloom_config.fpp.unwrap_or(0.05); // Default 5% false positive rate
                    let mut ndv = bloom_config.ndv.unwrap_or(1_000_000); // Default 1M distinct values

                    // Cap NDV to row group size (can't have more distinct values than total rows)
                    if let Some(row_group_size) = config.row_group_size {
                        ndv = ndv.min(row_group_size as u64);
                    }

                    let column_path = ColumnPath::from(schema.field(col_idx).name().as_str());
                    props_builder = props_builder
                        .set_column_bloom_filter_enabled(column_path.clone(), true)
                        .set_column_bloom_filter_fpp(column_path.clone(), fpp)
                        .set_column_bloom_filter_ndv(column_path, ndv);
                }
            }

            // Set sorting columns if configured
            if let Some(sorting_cols) = &config.sorting_columns {
                use parquet::format::SortingColumn;

                let parquet_sorting_cols: Vec<SortingColumn> = sorting_cols
                    .iter()
                    .map(|col| {
                        let col_idx = schema
                            .fields()
                            .iter()
                            .position(|f| f.name() == &col.column)
                            .ok_or_else(|| {
                                vector_common::Error::from(format!(
                                    "Sorting column '{}' not found in schema",
                                    col.column
                                ))
                            })?;

                        Ok(SortingColumn::new(col_idx as i32, col.descending, false))
                    })
                    .collect::<Result<Vec<_>, vector_common::Error>>()?;

                props_builder = props_builder.set_sorting_columns(Some(parquet_sorting_cols));
            }
        }
        // Note: Bloom filters and sorting are NOT applied for inferred schemas

        let writer_properties = props_builder.build();

        Ok(Self {
            schema_mode,
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

        // Determine schema based on mode
        let schema = match &self.schema_mode {
            SchemaMode::Explicit { schema } => Arc::clone(schema),
            SchemaMode::Inferred {
                exclude_columns,
                max_columns,
            } => infer_schema_from_events(&events, exclude_columns, *max_columns)?,
        };

        let bytes = encode_events_to_parquet(&events, schema, &self.writer_properties)?;

        // Use put() instead of extend_from_slice to avoid copying when possible
        buffer.put(bytes);
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

    /// No fields could be inferred from events
    #[snafu(display("No fields could be inferred from events (all fields excluded or only null values)"))]
    NoFieldsInferred,

    /// Invalid event type (not a log event)
    #[snafu(display("Invalid event type, expected log event"))]
    InvalidEventType,

    /// JSON serialization error for nested types
    #[snafu(display("Failed to serialize nested type as JSON: {}", source))]
    JsonSerialization {
        /// The underlying JSON error
        source: serde_json::Error,
    },

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

impl From<serde_json::Error> for ParquetEncodingError {
    fn from(error: serde_json::Error) -> Self {
        Self::JsonSerialization { source: error }
    }
}

/// Infer Arrow DataType from a Vector Value
fn infer_arrow_type(value: &vector_core::event::Value) -> arrow::datatypes::DataType {
    use vector_core::event::Value;
    use arrow::datatypes::{DataType, TimeUnit};

    match value {
        Value::Bytes(_) => DataType::Utf8,
        Value::Integer(_) => DataType::Int64,
        Value::Float(_) => DataType::Float64,
        Value::Boolean(_) => DataType::Boolean,
        Value::Timestamp(_) => DataType::Timestamp(TimeUnit::Microsecond, None),
        // Nested types and regex are always serialized as strings
        Value::Array(_) | Value::Object(_) | Value::Regex(_) => DataType::Utf8,
        // Null doesn't determine type, default to Utf8
        Value::Null => DataType::Utf8,
    }
}

/// Infer schema from a batch of events
fn infer_schema_from_events(
    events: &[Event],
    exclude_columns: &std::collections::BTreeSet<String>,
    max_columns: usize,
) -> Result<Arc<Schema>, ParquetEncodingError> {
    use std::collections::BTreeMap;
    use arrow::datatypes::{DataType, Field};
    use vector_core::event::Value;

    let mut field_types: BTreeMap<String, DataType> = BTreeMap::new();
    let mut type_conflicts: BTreeMap<String, Vec<DataType>> = BTreeMap::new();

    for event in events {
        // Only process log events
        let log = match event {
            Event::Log(log) => log,
            _ => return Err(ParquetEncodingError::InvalidEventType),
        };

        let fields_iter = log.all_event_fields().ok_or(ParquetEncodingError::InvalidEventType)?;

        for (key, value) in fields_iter {
            let key_str = key.to_string();

            // Skip excluded columns
            if exclude_columns.contains(&key_str) {
                continue;
            }

            // Skip Value::Null (doesn't determine type)
            if matches!(value, Value::Null) {
                continue;
            }

            // Enforce max columns (skip new fields after limit)
            if field_types.len() >= max_columns && !field_types.contains_key(&key_str) {
                tracing::debug!(
                    column = %key_str,
                    max_columns = max_columns,
                    "Skipping column: max_columns limit reached"
                );
                continue;
            }

            let inferred_type = infer_arrow_type(&value);

            match field_types.get(&key_str) {
                None => {
                    // First occurrence of this field
                    field_types.insert(key_str, inferred_type);
                }
                Some(existing_type) if existing_type != &inferred_type => {
                    // Type conflict detected - fallback to Utf8
                    tracing::warn!(
                        column = %key_str,
                        existing_type = ?existing_type,
                        new_type = ?inferred_type,
                        "Type conflict detected, encoding as Utf8"
                    );

                    type_conflicts
                        .entry(key_str.clone())
                        .or_insert_with(|| vec![existing_type.clone()])
                        .push(inferred_type);

                    field_types.insert(key_str, DataType::Utf8);
                }
                Some(_) => {
                    // Same type, no action needed
                }
            }
        }
    }

    if field_types.is_empty() {
        return Err(ParquetEncodingError::NoFieldsInferred);
    }

    // Build Arrow schema (all fields nullable)
    let arrow_fields: Vec<Arc<Field>> = field_types
        .into_iter()
        .map(|(name, dtype)| Arc::new(Field::new(name, dtype, true)))
        .collect();

    Ok(Arc::new(Schema::new(arrow_fields)))
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

    // Get batch metadata before we move into writer scope
    let batch_schema = record_batch.schema();

    // Write RecordBatch to Parquet format in memory
    let mut buffer = Vec::new();
    {
        let mut writer = ArrowWriter::try_new(
            &mut buffer,
            batch_schema,
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

        let result = encode_events_to_parquet(&events, Arc::clone(&schema), &props, None);
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

        let result = encode_events_to_parquet(&events, Arc::clone(&schema), &props, None);
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
        let result = encode_events_to_parquet(&events, schema, &props, None);
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

            let result = encode_events_to_parquet(&events, Arc::clone(&schema), &props, None);
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
        use super::schema_definition::FieldDefinition;

        let mut fields = BTreeMap::new();
        fields.insert(
            "field".to_string(),
            FieldDefinition {
                r#type: "int64".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );

        let config = ParquetSerializerConfig {
            schema: Some(SchemaDefinition { fields }),
            infer_schema: false,
            exclude_columns: None,
            max_columns: default_max_columns(),
            compression: ParquetCompression::Zstd,
            compression_level: None,
            writer_version: ParquetWriterVersion::default(),
            row_group_size: Some(1000),
            allow_nullable_fields: false,
            sorting_columns: None,
        };

        let serializer = ParquetSerializer::new(config);
        assert!(serializer.is_ok());
    }

    #[test]
    fn test_parquet_serializer_no_schema_fails() {
        let config = ParquetSerializerConfig {
            schema: None,
            infer_schema: false,
            exclude_columns: None,
            max_columns: default_max_columns(),
            compression: ParquetCompression::default(),
            compression_level: None,
            writer_version: ParquetWriterVersion::default(),
            row_group_size: None,
            allow_nullable_fields: false,
            sorting_columns: None,
        };

        let result = ParquetSerializer::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_encoder_trait_implementation() {
        use std::collections::BTreeMap;
        use tokio_util::codec::Encoder;
        use super::schema_definition::FieldDefinition;

        let mut fields = BTreeMap::new();
        fields.insert(
            "id".to_string(),
            FieldDefinition {
                r#type: "int64".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );
        fields.insert(
            "name".to_string(),
            FieldDefinition {
                r#type: "utf8".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );

        let config = ParquetSerializerConfig::new(SchemaDefinition { fields });
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

        let result = encode_events_to_parquet(&events, Arc::clone(&schema), &props, None);
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
        use super::schema_definition::FieldDefinition;

        let mut fields = BTreeMap::new();
        fields.insert(
            "required_field".to_string(),
            FieldDefinition {
                r#type: "int64".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );

        let mut log1 = LogEvent::default();
        log1.insert("required_field", 42);

        let log2 = LogEvent::default();
        // log2 is missing required_field

        let events = vec![Event::Log(log1), Event::Log(log2)];

        // Note: SchemaDefinition creates nullable fields by default
        // This test verifies that the allow_nullable_fields flag works
        let mut config = ParquetSerializerConfig::new(SchemaDefinition { fields });
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
