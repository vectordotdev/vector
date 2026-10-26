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
use bytes::{BufMut, BytesMut};
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
#[configurable(metadata(
    docs::enum_tag_description = "Compression codec applied per column page inside the Parquet file."
))]
#[serde(tag = "algorithm", rename_all = "snake_case")]
pub enum ParquetCompression {
    /// Zstd compression. Level must be between 1 and 21.
    Zstd {
        /// Compression level (1–21). This is the range Vector supports; higher values compress more but are slower.
        #[configurable(validation(range(min = 1, max = 21)))]
        level: u8,
    },
    /// Gzip compression. Level must be between 1 and 9.
    Gzip {
        /// Compression level (1–9). This is the range Vector supports; higher values compress more but are slower.
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
    pub compression: ParquetCompression,

    /// Controls how events with fields not present in the schema are handled.
    #[serde(default)]
    pub schema_mode: ParquetSchemaMode,

    /// The maximum number of rows in a row group.
    ///
    /// Defaults to the Parquet writer default of 1048576 rows.
    #[serde(default)]
    #[configurable(metadata(docs::type_unit = "events"))]
    pub max_row_group_rows: Option<usize>,

    /// The maximum estimated encoded size of a row group.
    ///
    /// A row group is written out once its encoded size reaches this limit, so lower
    /// values bound the memory held by an open file at the cost of more, smaller row
    /// groups, which are less efficient to query. Unset by default, in which case
    /// row groups are limited only by `max_row_group_rows`.
    #[serde(default)]
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub max_row_group_bytes: Option<usize>,
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
    #[must_use]
    pub fn input_type(&self) -> vector_core::config::DataType {
        vector_core::config::DataType::Log
    }

    /// The schema required by the serializer.
    #[must_use]
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
            field.name().clone()
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
#[derive(derive_more::Debug, Clone)]
pub struct ParquetSerializer {
    schema: SchemaRef,
    writer_props: Arc<WriterProperties>,
    schema_mode: ParquetSchemaMode,
    /// Pre-built set of schema field names for O(1) strict-mode lookups.
    schema_field_names: Arc<HashSet<String>>,

    #[debug(skip)]
    events_dropped_handle: Registered<EventsDroppedError>,
}

impl ParquetSerializer {
    /// Create a new `ParquetSerializer` from the given configuration.
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_errors_doc,
        reason = "The codec API error documentation needs a separate audit."
    )]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "Keep the existing owned-argument API during the lint rollout."
    )]
    pub fn new(
        config: ParquetSerializerConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let schema = config.resolve_schema()?;
        reject_unsupported_arrow_types(&schema)?;
        let schema_ref = SchemaRef::new(schema);

        let schema_field_names = Arc::new(
            schema_ref
                .fields()
                .iter()
                .map(|f| f.name().clone())
                .collect::<HashSet<_>>(),
        );

        if config.max_row_group_rows == Some(0) {
            return Err("max_row_group_rows must be greater than 0".into());
        }
        if config.max_row_group_bytes == Some(0) {
            return Err("max_row_group_bytes must be greater than 0".into());
        }

        let mut props = WriterProperties::builder().set_compression(config.compression.try_into()?);
        if let Some(rows) = config.max_row_group_rows {
            props = props.set_max_row_group_row_count(Some(rows));
        }
        if let Some(bytes) = config.max_row_group_bytes {
            props = props.set_max_row_group_bytes(Some(bytes));
        }
        let writer_props = Arc::new(props.build());

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
    #[must_use]
    pub const fn content_type(&self) -> &'static str {
        "application/vnd.apache.parquet"
    }

    /// Opens a new Parquet file that is encoded incrementally.
    ///
    /// `first_chunk` is used to infer the file schema when `schema_mode` is
    /// `auto_infer`; every later chunk written to the returned
    /// [`ParquetFileWriter`] must fit that schema. With a configured schema the
    /// chunk is only inspected for the usual `strict`/`relaxed` handling when it
    /// is written.
    ///
    /// # Errors
    ///
    /// Returns an error if the schema cannot be inferred from `first_chunk` or
    /// the underlying Parquet writer cannot be created.
    pub fn open_file(&self, first_chunk: &[Event]) -> Result<ParquetFileWriter, Error> {
        self.open_file_with_capacity(first_chunk, 0)
    }

    /// Like [`open_file`](Self::open_file), but reserves `capacity` bytes for the
    /// output up front.
    ///
    /// Pass the expected final size when it is known (for example the size limit at
    /// which the file is completed) so the buffer does not have to grow, and copy
    /// its contents, as row groups are flushed into it.
    ///
    /// # Errors
    ///
    /// See [`open_file`](Self::open_file).
    pub fn open_file_with_capacity(
        &self,
        first_chunk: &[Event],
        capacity: usize,
    ) -> Result<ParquetFileWriter, Error> {
        let schema = if self.schema_mode == ParquetSchemaMode::AutoInfer {
            let json_values = vector_log_events_to_json_values(first_chunk).map_err(|e| {
                emit(JsonSerializationError { error: &e });
                Error::new(ErrorKind::InvalidData, e)
            })?;
            Self::infer_file_schema(first_chunk, &json_values)?
        } else {
            Arc::clone(&self.schema)
        };
        self.open_file_with_schema(schema, capacity)
    }

    fn infer_file_schema(
        events: &[Event],
        json_values: &[serde_json::Value],
    ) -> Result<SchemaRef, Error> {
        if json_values.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Cannot infer a Parquet schema without any log events",
            ));
        }
        let schema = ParquetSchemaGenerator::infer_schema(json_values)?;
        Ok(Arc::new(ParquetSchemaGenerator::try_normalize_schema(
            events, schema,
        )))
    }

    fn open_file_with_schema(
        &self,
        schema: SchemaRef,
        capacity: usize,
    ) -> Result<ParquetFileWriter, Error> {
        let writer = ArrowWriter::try_new(
            BytesMut::with_capacity(capacity).writer(),
            Arc::clone(&schema),
            Some((*self.writer_props).clone()),
        )
        .map_err(|e| {
            emit(ArrowWriterError { error: &e });
            Error::other(e)
        })?;

        Ok(ParquetFileWriter {
            writer,
            schema,
            schema_mode: self.schema_mode,
            schema_field_names: Arc::clone(&self.schema_field_names),
            events_dropped_handle: self.events_dropped_handle.clone(),
            rows: 0,
        })
    }
}

/// Error returned by [`ParquetFileWriter::write`].
#[derive(Debug)]
pub enum ParquetWriteError {
    /// The chunk could not be turned into a record batch, for example because
    /// of a schema violation. The file is unaffected and accepts further chunks;
    /// only the events of this chunk are lost.
    Chunk(vector_common::Error),
    /// The Parquet writer itself failed. The file can no longer be completed
    /// and must be discarded along with everything written to it.
    File(parquet::errors::ParquetError),
}

impl std::fmt::Display for ParquetWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chunk(error) => write!(f, "Failed to encode events as Parquet: {error}"),
            Self::File(error) => write!(f, "Parquet writer failed: {error}"),
        }
    }
}

impl std::error::Error for ParquetWriteError {}

/// A Parquet file that is encoded one chunk of events at a time.
///
/// Each [`write`](Self::write) converts its chunk into an Arrow record batch and
/// hands it to the Parquet writer, which encodes and compresses the pages right
/// away and flushes completed row groups into the output buffer. Memory usage
/// therefore tracks the compressed output plus a single chunk, rather than the
/// uncompressed contents of the whole file as with [`ParquetSerializer::encode`].
///
/// Call [`encoded_size`](Self::encoded_size) after each write to decide when the
/// file is large enough, then [`finish`](Self::finish) to obtain the bytes.
#[derive(derive_more::Debug)]
pub struct ParquetFileWriter {
    #[debug(skip)]
    writer: ArrowWriter<bytes::buf::Writer<BytesMut>>,
    schema: SchemaRef,
    schema_mode: ParquetSchemaMode,
    schema_field_names: Arc<HashSet<String>>,
    #[debug(skip)]
    events_dropped_handle: Registered<EventsDroppedError>,
    rows: usize,
}

impl ParquetFileWriter {
    /// Encodes a chunk of events into the file.
    ///
    /// Returns the number of rows written. Non-log events are dropped and
    /// reported, as in [`ParquetSerializer::encode`].
    ///
    /// # Errors
    ///
    /// See [`ParquetWriteError`] for which errors leave the file usable.
    pub fn write(&mut self, events: &[Event]) -> Result<usize, ParquetWriteError> {
        let json_values = vector_log_events_to_json_values(events).map_err(|e| {
            emit(JsonSerializationError { error: &e });
            ParquetWriteError::Chunk(Box::new(e))
        })?;
        self.write_json_values(events, &json_values)
    }

    fn write_json_values(
        &mut self,
        events: &[Event],
        json_values: &[serde_json::Value],
    ) -> Result<usize, ParquetWriteError> {
        let non_log_count = events.len() - json_values.len();
        if non_log_count > 0 {
            warn!(
                message = "Non-log events dropped by Parquet encoder ",
                %non_log_count,
                internal_log_rate_secs = 10,
            );
            self.events_dropped_handle.emit(Count(non_log_count));
        }

        if json_values.is_empty() {
            return Ok(0);
        }

        if self.schema_mode == ParquetSchemaMode::Strict {
            self.check_strict_fields(events)?;
        }

        let record_batch = build_record_batch(Arc::clone(&self.schema), json_values)
            .map_err(|e| ParquetWriteError::Chunk(Box::new(e)))?;

        self.writer.write(&record_batch).map_err(|e| {
            emit(ArrowWriterError { error: &e });
            ParquetWriteError::File(e)
        })?;

        let rows = record_batch.num_rows();
        self.rows += rows;
        Ok(rows)
    }

    /// In strict mode, reject events with top-level fields not in the schema.
    fn check_strict_fields(&self, events: &[Event]) -> Result<(), ParquetWriteError> {
        for event in events {
            if let Some(log) = event.maybe_as_log()
                && let Some(object_map) = log.as_map()
            {
                for top_level in object_map.keys() {
                    if !self.schema_field_names.contains(top_level.as_str()) {
                        return Err(ParquetWriteError::Chunk(Box::new(
                            ArrowEncodingError::SchemaFetchError {
                                message: format!(
                                    "Strict schema mode: event contains field '{top_level}' not in schema",
                                ),
                            },
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// The Arrow schema of this file.
    #[must_use]
    pub fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    /// Number of rows written so far.
    #[must_use]
    pub const fn rows(&self) -> usize {
        self.rows
    }

    /// Estimated size of the finished file so far: bytes already flushed to the
    /// output buffer plus the estimated encoded size of the in-progress row group.
    #[must_use]
    pub fn encoded_size(&self) -> usize {
        self.writer.bytes_written() + self.writer.in_progress_size()
    }

    /// Writes the footer and returns the complete Parquet file.
    ///
    /// # Errors
    ///
    /// Returns an error if the Parquet writer fails to finalize the file.
    pub fn finish(self) -> Result<BytesMut, parquet::errors::ParquetError> {
        let writer = self
            .writer
            .into_inner()
            .inspect_err(|e| emit(ArrowWriterError { error: e }))?;
        Ok(writer.into_inner())
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
                emit(JsonSerializationError { error: &e });
                return Err(Box::new(e));
            }
        };

        let schema = if self.schema_mode == ParquetSchemaMode::AutoInfer {
            if json_values.is_empty() {
                // Only non-log events: report them as dropped and emit nothing.
                self.events_dropped_handle.emit(Count(events.len()));
                return Ok(());
            }
            let schema = Self::infer_file_schema(&events, &json_values)?;
            self.schema = Arc::clone(&schema);
            schema
        } else {
            Arc::clone(&self.schema)
        };

        let mut file = self.open_file_with_schema(schema, 0)?;
        let rows = file.write_json_values(&events, &json_values)?;
        if rows == 0 {
            return Ok(());
        }

        let output = file.finish().map_err(Box::new)?;
        buffer.unsplit(output);

        Ok(())
    }
}

pub struct ParquetSchemaGenerator {}

impl ParquetSchemaGenerator {
    pub fn infer_schema(events: &[serde_json::Value]) -> Result<Schema, Error> {
        let schema = infer_json_schema_from_iterator(events.iter().map(Ok::<_, ArrowError>))
            .map_err(|e| {
                emit(SchemaGenerationError { error: &e });
                Error::new(ErrorKind::InvalidData, e.to_string())
            })?;

        Ok(schema)
    }

    /// Attempt to modify schema to set timestamp fields as Timestamp instead of Utf8.
    /// Only works for top-level fields.
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::needless_pass_by_value,
        reason = "Keep the existing owned-argument API during the lint rollout."
    )]
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
    use vrl::event_path;

    fn create_event<V>(fields: Vec<(&str, V)>) -> Event
    where
        V: Into<vector_core::event::Value>,
    {
        let mut log = LogEvent::default();
        for (key, value) in fields {
            log.insert(&vrl::path::parse_target_path(key).unwrap(), value.into());
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
        log.insert(event_path!("host"), "localhost");
        log.insert(event_path!("message"), message);
        log.insert(event_path!("service"), "vector");
        log.insert(event_path!("source_type"), "demo_logs");
        log.insert(event_path!("timestamp"), Value::Timestamp(timestamp));
        log.insert(event_path!("random_time"), Value::Timestamp(timestamp));
        log.insert(event_path!("status_code"), Value::Integer(status_code));
        log.insert(event_path!("response_time_secs"), response_time_secs);
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
            "vector_parquet_test_{}_{name}.schema",
            std::process::id(),
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
                .unwrap_or_else(|e| panic!("Encoding with {compression:?} failed: {e}"));

            let data = buffer.freeze();
            assert_parquet_magic(&data);
            assert_eq!(
                parquet_row_count(&data),
                1,
                "Wrong row count for {compression:?}"
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
        log.insert(event_path!("name"), "alice");

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
        log.insert(event_path!("name"), "test");
        log.insert(event_path!("level"), "info");

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
    fn test_parquet_type_mismatch_returns_error() {
        let schema_path =
            write_temp_schema("type_mismatch", "message logs {\n  required int64 name;\n}");

        let mut serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_file: Some(schema_path),
            schema_mode: ParquetSchemaMode::Relaxed,
            ..Default::default()
        })
        .expect("Failed to create serializer");

        let events = vec![create_event(vec![("name", "not_an_integer")])];
        let mut buffer = BytesMut::new();
        let result = serializer.encode(events, &mut buffer);
        assert!(result.is_err(), "Type mismatch should return an error");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Int64"),
            "Error should mention the expected type, got: {err}"
        );
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

    #[test]
    fn row_group_limits_are_applied() {
        let serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_mode: ParquetSchemaMode::AutoInfer,
            max_row_group_rows: Some(2),
            ..Default::default()
        })
        .unwrap();
        let events = sample_events();
        let mut file = serializer.open_file(&events).unwrap();
        file.write(&events).unwrap();
        let output = file.finish().unwrap();
        let reader = SerializedFileReader::new(Bytes::from(output)).unwrap();
        assert_eq!(
            reader.metadata().num_row_groups(),
            3,
            "5 rows in groups of 2"
        );

        for config in [
            ParquetSerializerConfig {
                schema_mode: ParquetSchemaMode::AutoInfer,
                max_row_group_rows: Some(0),
                ..Default::default()
            },
            ParquetSerializerConfig {
                schema_mode: ParquetSchemaMode::AutoInfer,
                max_row_group_bytes: Some(0),
                ..Default::default()
            },
        ] {
            assert!(ParquetSerializer::new(config).is_err());
        }
    }

    // ── Incremental file writer ──────────────────────────────────────────────

    #[test]
    fn file_writer_accumulates_chunks_into_one_file() {
        let serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_mode: ParquetSchemaMode::AutoInfer,
            compression: ParquetCompression::Zstd { level: 3 },
            ..Default::default()
        })
        .expect("Failed to create serializer");

        // Distinct payloads per chunk so the encoded size visibly grows.
        let chunks: Vec<Vec<Event>> = (0..3)
            .map(|chunk| {
                (0..200)
                    .map(|i| {
                        create_event(vec![
                            (
                                "message",
                                format!("chunk {chunk} event {i} {}", "x".repeat(i)),
                            ),
                            ("host", format!("host-{i}")),
                        ])
                    })
                    .collect()
            })
            .collect();
        let chunk_len = chunks[0].len();

        let mut file = serializer
            .open_file(&chunks[0])
            .expect("open_file should succeed");
        assert_eq!(file.rows(), 0);

        let mut last_size = file.encoded_size();
        for chunk in &chunks {
            let rows = file.write(chunk).expect("write should succeed");
            assert_eq!(rows, chunk_len);
            let size = file.encoded_size();
            assert!(size > last_size, "encoded_size should grow with each chunk");
            last_size = size;
        }
        assert_eq!(file.rows(), chunk_len * 3);

        let output = file.finish().expect("finish should succeed");
        assert_parquet_magic(&output);
        assert_eq!(parquet_row_count(&output), chunk_len * 3);
        assert!(
            parquet_column_names(&output).contains(&"message".to_string()),
            "columns should come from the inferred schema"
        );
    }

    #[test]
    fn file_writer_matches_single_shot_encode() {
        let config = ParquetSerializerConfig {
            schema_mode: ParquetSchemaMode::AutoInfer,
            ..Default::default()
        };
        let events = sample_events();

        let mut serializer = ParquetSerializer::new(config.clone()).unwrap();
        let mut buffer = BytesMut::new();
        serializer.encode(events.clone(), &mut buffer).unwrap();

        let serializer = ParquetSerializer::new(config).unwrap();
        let mut file = serializer.open_file(&events).unwrap();
        file.write(&events).unwrap();
        let output = file.finish().unwrap();

        assert_eq!(parquet_row_count(&buffer), parquet_row_count(&output));
        assert_eq!(parquet_column_names(&buffer), parquet_column_names(&output));
    }

    #[test]
    fn file_writer_auto_infer_requires_log_events() {
        let serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_mode: ParquetSchemaMode::AutoInfer,
            ..Default::default()
        })
        .unwrap();

        let metric = Event::Metric(vector_core::event::Metric::new(
            "cpu",
            vector_core::event::MetricKind::Absolute,
            vector_core::event::MetricValue::Gauge { value: 1.0 },
        ));
        assert!(serializer.open_file(&[metric]).is_err());
        assert!(serializer.open_file(&[]).is_err());
    }

    #[test]
    fn file_writer_relaxed_schema_file_fills_missing_fields() {
        let schema_path = write_temp_schema(
            "file_writer_relaxed",
            "message logs {\n  required binary name (STRING);\n  optional int64 age;\n}",
        );
        let serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_file: Some(schema_path),
            schema_mode: ParquetSchemaMode::Relaxed,
            ..Default::default()
        })
        .unwrap();

        // With a schema file the first chunk is not needed for inference.
        let mut file = serializer.open_file(&[]).unwrap();
        file.write(&[create_event(vec![("name", "alice"), ("city", "paris")])])
            .unwrap();
        file.write(&[create_event(vec![("name", "bob")])]).unwrap();
        assert_eq!(file.rows(), 2);

        let output = file.finish().unwrap();
        assert_eq!(parquet_row_count(&output), 2);
        assert_eq!(parquet_column_names(&output), vec!["name", "age"]);
    }

    #[test]
    fn file_writer_strict_rejects_chunk_but_keeps_file_usable() {
        let schema_path = write_temp_schema(
            "file_writer_strict",
            "message logs {\n  required binary name (STRING);\n}",
        );
        let serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_file: Some(schema_path),
            schema_mode: ParquetSchemaMode::Strict,
            ..Default::default()
        })
        .unwrap();

        let mut file = serializer.open_file(&[]).unwrap();
        file.write(&[create_event(vec![("name", "alice")])])
            .unwrap();

        let err = file
            .write(&[create_event(vec![("name", "eve"), ("city", "paris")])])
            .expect_err("strict mode should reject extra fields");
        assert!(matches!(err, ParquetWriteError::Chunk(_)), "{err}");
        assert!(err.to_string().contains("city"));

        file.write(&[create_event(vec![("name", "bob")])]).unwrap();
        assert_eq!(file.rows(), 2);
        assert_eq!(parquet_row_count(&file.finish().unwrap()), 2);
    }

    #[test]
    fn file_writer_type_mismatch_is_chunk_error() {
        let schema_path = write_temp_schema(
            "file_writer_mismatch",
            "message logs {\n  required int64 count;\n}",
        );
        let serializer = ParquetSerializer::new(ParquetSerializerConfig {
            schema_file: Some(schema_path),
            ..Default::default()
        })
        .unwrap();

        let mut file = serializer.open_file(&[]).unwrap();
        file.write(&[create_event(vec![("count", 1_i64)])]).unwrap();
        let err = file
            .write(&[create_event(vec![("count", "not a number")])])
            .expect_err("type mismatch should fail");
        assert!(matches!(err, ParquetWriteError::Chunk(_)), "{err}");
        assert_eq!(parquet_row_count(&file.finish().unwrap()), 1);
    }
}
