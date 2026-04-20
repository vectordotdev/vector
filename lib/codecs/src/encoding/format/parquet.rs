//! Parquet batch format codec for batched event encoding
//!
//! Provides Apache Parquet format encoding with schema file support and auto-inference.
//! Reuses the Arrow record batch building logic from the Arrow IPC codec,
//! then writes the batch as a complete Parquet file using `ArrowWriter`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::error::ArrowError;
use arrow::json::reader::infer_json_schema_from_iterator;
use arrow::record_batch::RecordBatch;
use bytes::{BufMut, BytesMut};
use derivative::Derivative;
use parquet::arrow::ArrowWriter;
use parquet::basic::ZstdLevel;
use parquet::basic::{Compression as ParquetCodecCompression, GzipLevel};
use parquet::file::properties::WriterProperties;
use std::io::{Error, ErrorKind};
use tracing::warn;
use vector_common::internal_event::{
    ComponentEventsDropped, Count, InternalEventHandle, Registered, UNINTENTIONAL, emit, register,
};
use vector_config::configurable_component;
use vector_core::event::Event;

use super::arrow::{ArrowEncodingError, build_record_batch};
use crate::encoding::format::arrow::vector_log_events_to_json_values;
use crate::internal_events::{ArrowWriterError, JsonSerializationError, SchemaGenerationError};

type EventsDroppedError = ComponentEventsDropped<'static, UNINTENTIONAL>;

/// Compression algorithm and optional level for archive objects.
#[configurable_component]
#[derive(Default, Copy, Clone, Debug, PartialEq)]
#[serde(tag = "algorithm", rename_all = "snake_case")]
pub enum ParquetCompression {
    /// Zstd compression. Level must be between 1 and 21.
    Zstd {
        /// Compression level (1–21). This is the range Vector currently supports; higher values compress more but are slower.
        #[configurable(validation(range(min = 1, max = 21)))]
        level: u8,
    },
    /// Gzip compression. Level must be between 1 and 9.
    Gzip {
        /// Compression level (1–9). This is the range Vector currently supports; higher values compress more but are slower.
        #[configurable(validation(range(min = 1, max = 9)))]
        level: u8,
    },

    /// Snappy compression (no level).
    #[default]
    Snappy,

    /// LZ4 raw compression
    Lz4,

    /// No compression
    None,
}

impl TryFrom<ParquetCompression> for ParquetCodecCompression {
    type Error = parquet::errors::ParquetError;
    fn try_from(
        value: ParquetCompression,
    ) -> Result<ParquetCodecCompression, parquet::errors::ParquetError> {
        match value {
            ParquetCompression::None => Ok(ParquetCodecCompression::UNCOMPRESSED),
            ParquetCompression::Snappy => Ok(ParquetCodecCompression::SNAPPY),
            ParquetCompression::Zstd { level } => Ok(ParquetCodecCompression::ZSTD(
                ZstdLevel::try_new(level.into())?,
            )),
            ParquetCompression::Gzip { level } => Ok(ParquetCodecCompression::GZIP(
                GzipLevel::try_new(level.into())?,
            )),
            ParquetCompression::Lz4 => Ok(ParquetCodecCompression::LZ4_RAW),
        }
    }
}

/// Schema handling mode.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParquetSchemaMode {
    /// Missing fields become null. Extra fields are silently dropped.
    #[default]
    Relaxed,
    /// Missing fields become null. Extra fields cause an error.
    Strict,
    /// Auto infer schema based on the batch. No schema file needed.
    AutoInfer,
}

/// Configuration for the Parquet serializer.
///
/// Encodes events as Apache Parquet columnar files, optimized for analytical queries
/// via Athena, Trino, Spark, and other columnar query engines.
///
/// Either `schema_file` must be provided, or `schema_mode` must be set to `auto_infer`.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct ParquetSerializerConfig {
    /// Path to a native Parquet schema file (`.schema`).
    ///
    /// Required unless `schema_mode` is `auto_infer`. The file must contain a valid
    /// Parquet message type definition.
    #[serde(default)]
    pub schema_file: Option<PathBuf>,

    /// Compression codec applied per column page inside the Parquet file.
    #[serde(default)]
    #[configurable(derived)]
    pub compression: ParquetCompression,

    /// Controls how events with fields not present in the schema are handled.
    #[serde(default)]
    #[configurable(derived)]
    pub schema_mode: ParquetSchemaMode,
}

impl ParquetSerializerConfig {
    /// Resolve the Arrow schema from the configured schema source.
    fn resolve_schema(&self) -> Result<Schema, Box<dyn std::error::Error + Send + Sync>> {
        if self.schema_mode == ParquetSchemaMode::AutoInfer {
            return Ok(Schema::empty());
        }

        let path = self
            .schema_file
            .as_ref()
            .ok_or("schema_file is required unless schema_mode is auto_infer")?;

        let content = read_schema_file(path, "schema_file")?;
        let parquet_type = parquet::schema::parser::parse_message_type(&content)
            .map_err(|e| format!("Failed to parse Parquet schema: {e}"))?;
        let schema_desc = parquet::schema::types::SchemaDescriptor::new(Arc::new(parquet_type));
        let arrow_schema = parquet::arrow::parquet_to_arrow_schema(&schema_desc, None)
            .map_err(|e| format!("Failed to convert Parquet schema to Arrow: {e}"))?;
        Ok(arrow_schema)
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

/// Check the resolved Arrow schema for data types unsupported by the JSON-based
/// encode path (`arrow::json::reader::ReaderBuilder`). Binary variants are
/// accepted by Parquet/Arrow at the schema level but the JSON decoder rejects
/// them at runtime, so we fail fast here at config time.
fn reject_unsupported_arrow_types(
    schema: &Schema,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    fn check_field(field: &Field, path: &str, bad: &mut Vec<String>) {
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
#[derive(Derivative)]
#[derivative(Debug, Clone)]
pub struct ParquetSerializer {
    schema: SchemaRef,
    writer_props: Arc<WriterProperties>,
    schema_mode: ParquetSchemaMode,
    /// Pre-built set of schema field names for O(1) strict-mode lookups.
    schema_field_names: HashSet<String>,

    #[derivative(Debug = "ignore")]
    events_dropped_handle: Registered<EventsDroppedError>,
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
                .set_compression(config.compression.try_into()?)
                .build(),
        );

        Ok(Self {
            schema: schema_ref,
            writer_props,
            schema_mode: config.schema_mode,
            schema_field_names,
            events_dropped_handle: register(EventsDroppedError::from(
                "Events could not be serialized to parquet",
            )),
        })
    }

    /// Returns the MIME content type for Parquet data.
    pub const fn content_type(&self) -> &'static str {
        "application/vnd.apache.parquet"
    }

    /// Writes `record_batch` into `buffer` as a complete Parquet file.
    ///
    /// On failure, emits an [`ArrowWriterError`] internal event (which also
    /// increments `component_errors_total` and emits the events-dropped metric)
    /// before returning the error.
    fn write_record_batch(
        &self,
        record_batch: &RecordBatch,
        buffer: &mut BytesMut,
        event_count: usize,
    ) -> Result<(), parquet::errors::ParquetError> {
        let mut writer = ArrowWriter::try_new(
            buffer.writer(),
            Arc::clone(record_batch.schema_ref()),
            Some((*self.writer_props).clone()),
        )
        .inspect_err(|e| {
            emit(ArrowWriterError {
                error: e,
                batch_count: event_count,
            });
        })?;

        writer.write(record_batch).inspect_err(|e| {
            emit(ArrowWriterError {
                error: e,
                batch_count: event_count,
            });
        })?;

        writer.close().inspect_err(|e| {
            emit(ArrowWriterError {
                error: e,
                batch_count: event_count,
            });
        })?;

        Ok(())
    }
}

impl tokio_util::codec::Encoder<Vec<Event>> for ParquetSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, events: Vec<Event>, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        if events.is_empty() {
            return Ok(());
        }

        let json_values = match vector_log_events_to_json_values(&events) {
            Ok(values) => values,
            Err(e) => {
                emit(JsonSerializationError {
                    error: &e,
                    batch_count: events.len(),
                });
                return Err(Box::new(e));
            }
        };

        let non_log_count = events.len() - json_values.len();

        if non_log_count > 0 {
            warn!(
                message = "Non-log events dropped by Parquet encoder ",
                %non_log_count,
                internal_log_rate_secs = 10,
            );
            self.events_dropped_handle.emit(Count(non_log_count))
        }

        if json_values.is_empty() {
            return Ok(());
        }

        match self.schema_mode {
            // In strict mode, check for extra top-level fields not in the schema.
            ParquetSchemaMode::Strict => {
                for event in &events {
                    if let Some(log) = event.maybe_as_log()
                        && let Some(object_map) = log.as_map()
                    {
                        for top_level in object_map.keys() {
                            if !self.schema_field_names.contains(top_level.as_str()) {
                                self.events_dropped_handle.emit(Count(events.len()));
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
            ParquetSchemaMode::AutoInfer => {
                let schema = ParquetSchemaGenerator::infer_schema(&json_values)?;
                self.schema = Arc::new(ParquetSchemaGenerator::try_normalize_schema(
                    &events, schema,
                ));
            }
            ParquetSchemaMode::Relaxed => {}
        }

        let record_batch =
            build_record_batch(Arc::clone(&self.schema), &json_values).map_err(Box::new)?;

        self.write_record_batch(&record_batch, buffer, json_values.len())
            .map_err(Box::new)?;

        Ok(())
    }
}

pub struct ParquetSchemaGenerator {}

impl ParquetSchemaGenerator {
    pub fn infer_schema(events: &[serde_json::Value]) -> Result<Schema, Error> {
        let schema = infer_json_schema_from_iterator(events.iter().map(Ok::<_, ArrowError>))
            .map_err(|e| {
                emit(SchemaGenerationError {
                    error: &e,
                    batch_count: events.len(),
                });
                Error::new(ErrorKind::InvalidData, e.to_string())
            })?;

        Ok(schema)
    }

    /// Attempt to modify schema to set timestamp fields as Timestamp instead of Utf8.
    /// Only works for top-level fields.
    fn try_normalize_schema(events: &[Event], schema: Schema) -> Schema {
        let mut ts_seen: HashSet<String> = HashSet::new();
        let mut non_ts_seen: HashSet<String> = HashSet::new();

        for event in events.iter().filter_map(Event::maybe_as_log) {
            if let Some(object_map) = event.as_map() {
                for (path, value) in object_map {
                    if value.is_timestamp() {
                        ts_seen.insert(path.to_string());
                    } else if !value.is_null() {
                        non_ts_seen.insert(path.to_string());
                    }
                }
            }
        }

        let new_fields: Vec<Field> = schema
            .fields()
            .iter()
            .map(|f| {
                if ts_seen.contains(f.name()) && !non_ts_seen.contains(f.name()) {
                    Field::new(
                        f.name(),
                        DataType::Timestamp(
                            arrow::datatypes::TimeUnit::Microsecond,
                            Some("UTC".into()),
                        ),
                        f.is_nullable(),
                    )
                } else {
                    f.as_ref().clone()
                }
            })
            .collect();

        Schema::new_with_metadata(new_fields, schema.metadata().clone())
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

    fn assert_parquet_magic(data: &[u8]) {
        assert!(data.len() >= 4, "Output too short to be valid Parquet");
        assert_eq!(&data[..4], b"PAR1", "Missing Parquet magic bytes");
    }

    fn parquet_row_count(data: &[u8]) -> usize {
        let reader =
            SerializedFileReader::new(Bytes::copy_from_slice(data)).expect("Invalid Parquet file");
        let iter = RowIter::from_file_into(Box::new(reader));
        iter.count()
    }

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

    fn parse_timestamp(s: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .expect("invalid test timestamp")
            .with_timezone(&chrono::Utc)
    }

    fn demo_log_event(
        message: &str,
        timestamp: chrono::DateTime<chrono::Utc>,
        status_code: i64,
        response_time_secs: f64,
    ) -> Event {
        use vector_core::event::Value;
        let mut log = LogEvent::default();
        log.insert("host", "localhost");
        log.insert("message", message);
        log.insert("service", "vector");
        log.insert("source_type", "demo_logs");
        log.insert("timestamp", Value::Timestamp(timestamp));
        log.insert("random_time", Value::Timestamp(timestamp));
        log.insert("status_code", Value::Integer(status_code));
        log.insert("response_time_secs", response_time_secs);
        Event::Log(log)
    }

    fn sample_events() -> Vec<Event> {
        const EVENTS: [(&str, &str, i64, f64); 5] = [
            (
                "GET /api/v1/health HTTP/1.1",
                "2026-03-05T20:49:08.037194Z",
                200,
                0.037,
            ),
            (
                "POST /api/v1/ingest HTTP/1.1",
                "2026-03-05T20:49:09.038051Z",
                201,
                0.013,
            ),
            (
                "GET /metrics HTTP/1.1",
                "2026-03-05T20:49:10.036612Z",
                200,
                0.022,
            ),
            (
                "DELETE /api/v1/resource HTTP/1.1",
                "2026-03-05T20:49:11.537131Z",
                404,
                0.005,
            ),
            (
                "PATCH /api/v1/config HTTP/1.1",
                "2026-03-05T20:49:12.037491Z",
                500,
                0.091,
            ),
        ];
        EVENTS
            .iter()
            .map(|(msg, ts, status, rt)| demo_log_event(msg, parse_timestamp(ts), *status, *rt))
            .collect()
    }

    fn encode_autoinfer_and_read_schema(
        events: Vec<Event>,
    ) -> (arrow::datatypes::SchemaRef, usize) {
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_mode: ParquetSchemaMode::AutoInfer,
            ..Default::default()
        })
        .expect("AutoInfer serializer should be created without a static schema");

        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("encoding should succeed");

        let data = buffer.freeze();
        assert_parquet_magic(&data);

        let builder = ParquetRecordBatchReaderBuilder::try_new(data)
            .expect("should build ParquetRecordBatchReaderBuilder");
        let schema = builder.schema().clone();
        let num_rows: usize = builder
            .build()
            .expect("should build reader")
            .map(|b| b.expect("batch read error").num_rows())
            .sum();
        (schema, num_rows)
    }

    /// Write a temporary Parquet schema file and return its path.
    ///
    /// `name` must be unique per test to avoid parallel-test races on the same file.
    fn write_temp_schema(name: &str, content: &str) -> std::path::PathBuf {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!(
            "vector_parquet_test_{}_{}.schema",
            std::process::id(),
            name,
        ));
        let mut f = std::fs::File::create(&path).expect("Failed to create schema file");
        write!(f, "{content}").expect("Failed to write schema");
        path
    }

    // ── AutoInfer mode ───────────────────────────────────────────────────────

    #[test]
    fn encode_input_produces_parquet_output() {
        let events = sample_events();
        let n_events = events.len();
        let (schema, num_rows) = encode_autoinfer_and_read_schema(events);

        assert_eq!(num_rows, n_events, "row count should match event count");

        for field_name in &["timestamp", "random_time"] {
            let field = schema
                .field_with_name(field_name)
                .unwrap_or_else(|_| panic!("field '{field_name}' should exist in schema"));
            assert!(
                matches!(
                    field.data_type(),
                    DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some(tz)) if tz.as_ref() == "UTC"
                ),
                "'{field_name}' should be Timestamp(Microsecond, UTC), got {:?}",
                field.data_type()
            );
        }

        let status_field = schema
            .field_with_name("status_code")
            .expect("status_code field should exist");
        assert_eq!(status_field.data_type(), &DataType::Int64);

        let rt_field = schema
            .field_with_name("response_time_secs")
            .expect("response_time_secs field should exist");
        assert_eq!(rt_field.data_type(), &DataType::Float64);

        for field_name in &["host", "message", "service", "source_type"] {
            let field = schema
                .field_with_name(field_name)
                .unwrap_or_else(|_| panic!("field '{field_name}' should exist in schema"));
            assert_eq!(field.data_type(), &DataType::Utf8);
        }
    }

    #[test]
    fn test_parquet_empty_events() {
        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_mode: ParquetSchemaMode::AutoInfer,
            ..Default::default()
        })
        .expect("AutoInfer serializer should succeed");

        let events: Vec<Event> = vec![];
        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("Empty events should succeed");

        assert!(buffer.is_empty(), "Buffer should be empty for empty events");
    }

    #[test]
    fn test_parquet_compression_variants() {
        let events = vec![create_event(vec![("msg", "hello world")])];

        let compressions = vec![
            ParquetCompression::None,
            ParquetCompression::Snappy,
            ParquetCompression::Zstd { level: 1 },
            ParquetCompression::Gzip { level: 1 },
            ParquetCompression::Lz4,
        ];

        for compression in compressions {
            let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
                schema_mode: ParquetSchemaMode::AutoInfer,
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
    fn test_parquet_output_has_footer() {
        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_mode: ParquetSchemaMode::AutoInfer,
            ..Default::default()
        })
        .expect("AutoInfer serializer should succeed");

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

    #[test]
    fn test_writer_props_arc_shared() {
        let serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_mode: ParquetSchemaMode::AutoInfer,
            ..Default::default()
        })
        .expect("AutoInfer serializer should succeed");
        let cloned = serializer.clone();

        assert_eq!(Arc::strong_count(&serializer.writer_props), 2);
        drop(cloned);
        assert_eq!(Arc::strong_count(&serializer.writer_props), 1);
    }

    #[test]
    fn test_mixed_log_and_non_log_events() {
        use vector_core::event::{Metric, MetricKind, MetricValue};

        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_mode: ParquetSchemaMode::AutoInfer,
            ..Default::default()
        })
        .expect("AutoInfer serializer should succeed");

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
        assert_eq!(parquet_row_count(&buffer), 2);
    }

    // ── Schema file mode ─────────────────────────────────────────────────────

    #[test]
    fn test_parquet_schema_file() {
        let schema_path = write_temp_schema(
            "schema_file",
            "message logs {\n  required binary name (STRING);\n  optional int64 age;\n}",
        );

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

    #[test]
    fn test_parquet_schema_file_invalid_syntax_error() {
        let schema_path = write_temp_schema(
            "invalid_syntax",
            "this is not valid parquet schema syntax !!!",
        );

        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema_file": schema_path.to_str().unwrap()
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
    fn test_parquet_no_schema_error() {
        let config = ParquetSerializerConfig::default();
        let result = ParquetSerializer::new(config);
        assert!(
            result.is_err(),
            "Should fail without schema_file or auto_infer"
        );
    }

    // ── Schema mode: strict / relaxed ────────────────────────────────────────

    #[test]
    fn test_parquet_strict_mode_rejects_extra_fields() {
        let schema_path = write_temp_schema(
            "strict_rejects",
            "message logs {\n  required binary name (STRING);\n}",
        );

        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_file: Some(schema_path),
            schema_mode: ParquetSchemaMode::Strict,
            ..Default::default()
        })
        .expect("Failed to create strict serializer");

        let events = vec![create_event(vec![("name", "alice"), ("city", "paris")])];
        let mut buffer = BytesMut::new();
        let result = serializer.encode(events, &mut buffer);
        assert!(result.is_err(), "Strict mode should reject extra fields");
        assert!(result.unwrap_err().to_string().contains("city"));
    }

    #[test]
    fn test_parquet_strict_mode_allows_schema_fields() {
        let schema_path = write_temp_schema(
            "strict_allows",
            "message logs {\n  required binary name (STRING);\n  required binary level (STRING);\n}",
        );

        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_file: Some(schema_path),
            schema_mode: ParquetSchemaMode::Strict,
            ..Default::default()
        })
        .expect("Failed to create strict serializer");

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
    fn test_parquet_relaxed_mode_drops_extra_fields() {
        let schema_path = write_temp_schema(
            "relaxed_drops",
            "message logs {\n  required binary name (STRING);\n}",
        );

        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_file: Some(schema_path),
            schema_mode: ParquetSchemaMode::Relaxed,
            ..Default::default()
        })
        .expect("Failed to create relaxed serializer");

        let events = vec![create_event(vec![("name", "alice"), ("city", "paris")])];
        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("Relaxed mode should drop extra fields silently");

        let data = buffer.freeze();
        assert_parquet_magic(&data);
        assert_eq!(parquet_row_count(&data), 1);
        let columns = parquet_column_names(&data);
        assert_eq!(columns, vec!["name"]);
    }

    #[test]
    fn test_parquet_schema_file_binary_without_string_annotation_rejected() {
        // Native Parquet "binary" without (STRING) annotation resolves to Arrow Binary,
        // which is rejected at config time.
        let schema_path = write_temp_schema(
            "binary_rejected",
            "message logs {\n  required binary name (STRING);\n  optional binary raw_data;\n}",
        );

        let config: ParquetSerializerConfig = serde_json::from_value(serde_json::json!({
            "schema_file": schema_path.to_str().unwrap()
        }))
        .expect("Config should deserialize");

        let result = ParquetSerializer::new(config);
        assert!(
            result.is_err(),
            "Parquet binary without STRING annotation should be rejected"
        );
        assert!(
            result.unwrap_err().to_string().contains("raw_data"),
            "Error should name the offending field"
        );
    }
}
