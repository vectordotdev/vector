use crate::config::{LogNamespace, SourceContext};
use crate::event::{Event, LogEvent};
use crate::internal_events::{EventsReceived, OdbcFailedError, OdbcQueryExecuted};
use crate::sinks::prelude::*;
use crate::sources::odbc::config::OdbcConfig;
use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use chrono_tz::Tz;
use futures::pin_mut;
use futures_util::StreamExt;
use itertools::Itertools;
use odbc_api::buffers::TextRowSet;
use odbc_api::parameter::VarCharBox;
use odbc_api::{
    ConnectionOptions, Cursor, Environment, IntoParameter, ResultSetMetadata, environment,
};
use snafu::{ResultExt, Snafu};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::mem;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio::select;
use vector_common::internal_event::{
    ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol, Registered,
};
use vector_common::json_size::JsonSize;
use vector_lib::EstimatedJsonEncodedSizeOf;
use vector_lib::emit;
use vector_lib::source_sender::SendError;
use vrl::prelude::*;

const TIMESTAMP_FORMATS: &[&str] = &[
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%dT%H:%M:%S",
    "%Y/%m/%d %H:%M:%S",
    "%Y/%m/%dT%H:%M:%S",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y/%m/%d %H:%M:%S%.f",
    "%Y/%m/%dT%H:%M:%S%.f",
];

struct Column {
    column_name: String,
    column_type: odbc_api::DataType,
}

/// Columns of the query result.
type Columns = Vec<Column>;
/// Rows of the query result.
type Rows = Vec<Value>;

#[derive(Debug, Snafu)]
pub enum OdbcError {
    #[snafu(display("ODBC database error: {source}"))]
    Db { source: odbc_api::Error },

    #[snafu(display("File IO error: {source}"))]
    Io { source: std::io::Error },

    #[snafu(display("Send error: {source}"))]
    SendError { source: SendError },

    #[snafu(display("JSON error: {source}"))]
    Json { source: serde_json::Error },

    #[snafu(display("Blocking ODBC task failed: {source}"))]
    BlockingTask { source: tokio::task::JoinError },

    #[snafu(display("Missing tracking column `{column}`"))]
    MissingTrackingColumn { column: String },

    #[snafu(display(
        "Tracking column `{column}` has a value that cannot be converted to an ODBC parameter"
    ))]
    InvalidTrackingValue { column: String },

    #[snafu(display("Last query result row is not an object; cannot extract tracking columns"))]
    InvalidTrackingRow,

    #[snafu(display("Query result row is not an object; cannot convert to a log event"))]
    InvalidResultRow,

    #[snafu(display(
        "Query returned duplicate column names: {columns:?}; alias columns in the SQL statement"
    ))]
    DuplicateColumnNames { columns: Vec<String> },
}

pub(crate) struct Context {
    cfg: OdbcConfig,
    env: &'static Environment,
    cx: SourceContext,
    log_namespace: LogNamespace,
}

impl Context {
    pub(crate) fn new(
        cfg: OdbcConfig,
        cx: SourceContext,
        log_namespace: LogNamespace,
    ) -> Result<Self, OdbcError> {
        let env = environment().context(DbSnafu)?;

        Ok(Self {
            cfg,
            env,
            cx,
            log_namespace,
        })
    }

    pub(crate) async fn run_schedule(self: Box<Self>) -> Result<(), ()> {
        let shutdown = self.cx.shutdown.clone();

        let schedule = self.cfg.schedule.clone().stream(self.cfg.schedule_timezone);
        pin_mut!(schedule);

        let bytes_received = register!(BytesReceived::from(Protocol::from("odbc")));
        let events_received = register!(EventsReceived);

        #[cfg(test)]
        let mut count = 0;

        let mut prev_result = self.cfg.statement_init_params.clone();

        loop {
            select! {
                _ = shutdown.clone() => {
                    debug!(message = "Shutdown signal received. Shutting down ODBC source.");
                    break;
                }
                next = schedule.next() => {
                    if next.is_none() {
                        debug!(message = "Schedule exhausted. Shutting down ODBC source.");
                        break;
                    }

                    let instant = Instant::now();
                    match self
                        .process(prev_result.clone(), &bytes_received, &events_received)
                        .await
                    {
                        Ok(result) => {
                            // Update the cached result when the query returns rows.
                            if result.is_some() {
                                prev_result = result;
                            }

                            emit!(OdbcQueryExecuted {
                                statement: &self.cfg.statement.clone().unwrap_or_default(),
                                elapsed: instant.elapsed().as_millis(),
                            });
                        }
                        Err(error) => {
                            emit!(OdbcFailedError {
                                statement: &self.cfg.statement.clone().unwrap_or_default(),
                                error,
                            });
                        }
                    }

                    #[cfg(test)]
                    {
                        count += 1;
                        if let Some(iterations) = self.cfg.iterations
                            && count >= iterations {
                                debug!(message = "No additional schedule configured. Shutting down ODBC source.");
                                break;
                            }
                    }
                }
            }
        }

        Ok(())
    }

    /// Executes the scheduled ODBC query, sends the result as events in bounded batches,
    /// then updates tracking metadata after all batches are successfully converted and sent.
    async fn process(
        &self,
        map: Option<ObjectMap>,
        bytes_received: &Registered<BytesReceived>,
        events_received: &Registered<EventsReceived>,
    ) -> Result<Option<ObjectMap>, OdbcError> {
        let conn_str = self.cfg.connection_string_or_file().context(IoSnafu)?;
        let stmt_str = self.cfg.statement_or_file().context(IoSnafu)?;
        if stmt_str.trim().is_empty() {
            return Err(OdbcError::Io {
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "either a non-empty `statement` or a readable `statement_filepath` must be provided",
                ),
            });
        }
        let out = self.cx.out.clone();
        let env = self.env;

        // Load the last-run metadata from disk when available.
        // If the file is missing, fall back to the initial parameters or the latest query result.
        // Unreadable or corrupt metadata is treated as an error to avoid replaying old rows.
        let tz = self.cfg.odbc_default_timezone;
        let fallback_map = map.unwrap_or_default();
        let stmt_params = if let Some(path) = &self.cfg.last_run_metadata_path {
            match load_params(path, self.cfg.tracking_columns.as_ref(), tz)? {
                Some(params) => params,
                None => order_params(&fallback_map, self.cfg.tracking_columns.as_ref(), tz)?,
            }
        } else {
            order_params(&fallback_map, self.cfg.tracking_columns.as_ref(), tz)?
        };
        let cfg = self.cfg.clone();
        let login_timeout = cfg.login_timeout;
        let statement_timeout = cfg.statement_timeout;
        let batch_size = cfg.odbc_batch_size;
        let max_str_limit = (cfg.odbc_max_str_limit > 0).then_some(cfg.odbc_max_str_limit);

        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let blocking = tokio::task::spawn_blocking(move || {
            let result = execute_query(
                env,
                &conn_str,
                &stmt_str,
                stmt_params,
                login_timeout,
                statement_timeout,
                tz,
                batch_size,
                max_str_limit,
                |batch| {
                    if tx.blocking_send(batch).is_err() {
                        return Ok(false);
                    }
                    Ok(true)
                },
            );
            drop(tx);
            result
        });

        // Last row of the most recently sent batch. Used for tracking only after every batch succeeds.
        let mut last_successfully_sent_row = None;
        let mut total_payload_byte_size = 0;
        let mut total_events = CountByteSize(0, JsonSize::zero());
        let mut stream_error = None;

        while let Some(batch_rows) = rx.recv().await {
            if batch_rows.is_empty() {
                continue;
            }

            // Keep the last raw row for tracking before rows are moved into events.
            let last_row = batch_rows.last().cloned();
            let mut events = match rows_to_events(batch_rows) {
                Ok(result) => result,
                Err(error) => {
                    stream_error = Some(error);
                    break;
                }
            };

            self.enrich_events(&mut events);

            let event_count = events.len();
            if event_count > 0 {
                // Use the post-enrichment size for both received-bytes and events metrics
                // so they stay consistent.
                let byte_size = events.estimated_json_encoded_size_of();
                if let Err(error) = out.clone().send_batch(events).await.context(SendSnafu) {
                    stream_error = Some(error);
                    break;
                }
                total_payload_byte_size += byte_size.get();
                total_events += CountByteSize(event_count, byte_size);
            }

            last_successfully_sent_row = last_row;
        }

        drop(rx);
        let blocking_result = blocking.await.context(BlockingTaskSnafu)?;

        if total_payload_byte_size > 0 {
            bytes_received.emit(ByteSize(total_payload_byte_size));
        }
        if total_events.0 > 0 {
            events_received.emit(total_events);
        }

        if let Some(error) = stream_error {
            let _ = blocking_result;
            return Err(error);
        }

        blocking_result?;

        // Advance tracking metadata only after all batches are converted and sent so a
        // partial failure does not skip rows on the next scheduled run.
        let mut latest_result = None;
        if let (Some(last), Some(tracking_columns)) =
            (last_successfully_sent_row, cfg.tracking_columns.clone())
        {
            latest_result = extract_and_save_tracking(
                cfg.last_run_metadata_path.as_deref(),
                last,
                tracking_columns,
                tz,
            )
            .await?;
        }

        Ok(latest_result)
    }

    fn enrich_events(&self, events: &mut [Event]) {
        let now = Utc::now();

        for event in events {
            let Event::Log(log) = event else {
                continue;
            };

            self.log_namespace
                .insert_standard_vector_source_metadata(log, OdbcConfig::NAME, now);
        }
    }
}

/// Converts ODBC result rows into log events without a JSON round-trip so typed
/// values such as timestamps and integers are preserved for downstream transforms.
fn rows_to_events(rows: Rows) -> Result<Vec<Event>, OdbcError> {
    let mut events = Vec::with_capacity(rows.len());

    for row in rows {
        let Value::Object(obj) = row else {
            return Err(OdbcError::InvalidResultRow);
        };

        events.push(LogEvent::from(obj).into());
    }

    Ok(events)
}

/// Extracts specified tracking columns from the given object,
/// saves them to a file if a path is provided.
///
/// Checkpoint values are stored as the exact SQL parameter text used for ODBC binding
/// so JSON roundtrips do not lose timestamp timezone formatting.
async fn extract_and_save_tracking(
    path: Option<&str>,
    obj: Value,
    tracking_columns: Vec<String>,
    tz: Tz,
) -> Result<Option<ObjectMap>, OdbcError> {
    let Value::Object(obj) = obj else {
        return Err(OdbcError::InvalidTrackingRow);
    };

    let mut save_obj = ObjectMap::new();
    for column in tracking_columns {
        let (_, param) = resolve_tracking_column_parameter(&obj, column.as_str(), tz)?;
        save_obj.insert(
            KeyString::from(column.as_str()),
            Value::Bytes(Bytes::from(param)),
        );
    }

    if let Some(path) = path {
        save_params(path, &save_obj)?;
    }
    Ok(Some(save_obj))
}

/// Returns an error when the query result contains duplicate column labels.
fn ensure_unique_column_names(names: &[String]) -> Result<(), OdbcError> {
    let mut seen = HashSet::with_capacity(names.len());
    let mut duplicates = Vec::new();

    for name in names {
        if !seen.insert(name.as_str()) {
            duplicates.push(name.clone());
        }
    }

    if duplicates.is_empty() {
        Ok(())
    } else {
        duplicates.sort_unstable();
        duplicates.dedup();
        Err(OdbcError::DuplicateColumnNames {
            columns: duplicates,
        })
    }
}

/// Executes an ODBC SQL query with optional parameters and invokes `on_batch` for each
/// fetched batch instead of accumulating the full result set in memory.
///
/// The callback returns `Ok(true)` to continue fetching or `Ok(false)` to stop early.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_query<F>(
    env: &Environment,
    conn_str: &str,
    stmt_str: &str,
    stmt_params: Vec<VarCharBox>,
    login_timeout: Duration,
    statement_timeout: Duration,
    tz: Tz,
    batch_size: usize,
    max_str_limit: Option<usize>,
    mut on_batch: F,
) -> Result<(), OdbcError>
where
    F: FnMut(Rows) -> Result<bool, OdbcError>,
{
    let conn_options = ConnectionOptions {
        login_timeout_sec: Some(login_timeout.as_secs() as u32),
        packet_size: None,
    };
    let conn = env
        .connect_with_connection_string(conn_str, conn_options)
        .context(DbSnafu)?;
    let mut statement = conn.preallocate().context(DbSnafu)?;
    statement
        .set_query_timeout_sec(statement_timeout.as_secs() as usize)
        .context(DbSnafu)?;

    let result = if stmt_params.is_empty() {
        statement.execute(stmt_str, ())
    } else {
        statement.execute(stmt_str, &stmt_params[..])
    }
    .context(DbSnafu)?;

    let Some(mut cursor) = result else {
        return Ok(());
    };

    let names = cursor
        .column_names()
        .context(DbSnafu)?
        .collect::<Result<Vec<String>, _>>()
        .context(DbSnafu)?;

    ensure_unique_column_names(&names)?;

    let types = (1..=names.len())
        .map(|col_index| cursor.col_data_type(col_index as u16).unwrap_or_default())
        .collect_vec();
    let columns = names
        .into_iter()
        .zip(types)
        .map(|(column_name, column_type)| Column {
            column_name,
            column_type,
        })
        .collect::<Columns>();

    let buffer = TextRowSet::for_cursor(batch_size, &mut cursor, max_str_limit).context(DbSnafu)?;
    let mut row_set_cursor = cursor.bind_buffer(buffer).context(DbSnafu)?;
    let mut batch_rows = Rows::with_capacity(batch_size);

    while let Some(batch) = row_set_cursor
        .fetch_with_truncation_check(true)
        .context(DbSnafu)?
    {
        let num_rows = batch.num_rows();

        for row_index in 0..num_rows {
            let mut cols = ObjectMap::new();

            for (index, column) in columns.iter().enumerate() {
                let data_name = &column.column_name;
                let data_type = &column.column_type;
                let data_value = batch.at(index, row_index);
                let key = KeyString::from(data_name.as_str());
                let value = map_value(data_type, data_value, tz);
                cols.insert(key, value);
            }

            batch_rows.push(Value::Object(cols));
        }

        if !batch_rows.is_empty() {
            if !on_batch(mem::take(&mut batch_rows))? {
                break;
            }
            batch_rows.reserve(batch_size);
        }
    }

    Ok(())
}

/// Loads the previously saved result and returns it as SQL parameters.
/// Parameters are generated in the order specified by `columns_order`.
///
/// Returns `Ok(None)` only when the metadata file does not exist.
fn load_params(
    path: &str,
    columns_order: Option<&Vec<String>>,
    tz: Tz,
) -> Result<Option<Vec<VarCharBox>>, OdbcError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(OdbcError::Io { source }),
    };
    let reader = BufReader::new(file);
    let map: ObjectMap = serde_json::from_reader(reader).context(JsonSnafu)?;

    order_params(&map, columns_order, tz).map(Some)
}

/// Orders the parameters of a given `ObjectMap` based on an optional column order.
///
/// When `columns_order` is set, every declared column must be present in `map` and
/// convertible to an ODBC parameter. Missing or unconvertible values return an error
/// instead of being silently dropped.
fn order_params(
    map: &ObjectMap,
    columns_order: Option<&Vec<String>>,
    tz: Tz,
) -> Result<Vec<VarCharBox>, OdbcError> {
    if columns_order.is_none_or(Vec::is_empty) {
        let params = map
            .values()
            .filter_map(|value| {
                value_to_sql_parameter(value, tz).map(|param| param.into_parameter())
            })
            .collect_vec();
        return Ok(params);
    }

    let columns = columns_order.expect("non-empty tracking_columns checked above");
    let mut params = Vec::with_capacity(columns.len());
    for column in columns {
        let (_, param) = resolve_tracking_column_parameter(map, column.as_str(), tz)?;
        params.push(param.into_parameter());
    }

    Ok(params)
}

/// Creates parent directories for the metadata path when needed.
pub(crate) fn prepare_metadata_path(path: &str) -> Result<(), OdbcError> {
    if path.is_empty() {
        return Err(OdbcError::Io {
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "`last_run_metadata_path` must not be empty",
            ),
        });
    }

    let path = Path::new(path);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    fs::create_dir_all(parent).context(IoSnafu)
}

/// Returns a sibling temp path that is always distinct from `path`.
///
/// Appending `.tmp` to the full filename avoids `Path::with_extension("tmp")`
/// returning the destination path when it already ends in `.tmp`.
fn checkpoint_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| format!("{}.tmp", name.to_string_lossy()))
        .unwrap_or_else(|| "checkpoint.tmp".into());
    path.with_file_name(file_name)
}

/// Replaces `path` with the contents of `tmp_path` atomically.
///
/// On Windows, `rename` cannot replace an existing destination file, so use
/// `ReplaceFileW` and fall back to `rename` when the destination does not exist yet.
fn replace_checkpoint_file(tmp_path: &Path, path: &Path) -> Result<(), OdbcError> {
    #[cfg(windows)]
    {
        use windows::Win32::Storage::FileSystem::ReplaceFileW;
        use windows::core::HSTRING;

        let dst = HSTRING::from(path.to_string_lossy().as_ref());
        let src = HSTRING::from(tmp_path.to_string_lossy().as_ref());
        let replaced = unsafe {
            ReplaceFileW(
                &dst,
                &src,
                None,
                windows::Win32::Storage::FileSystem::REPLACE_FILE_FLAGS(0),
                None,
                None,
            )
        };
        if replaced.is_ok() {
            return Ok(());
        }
        // Destination may not exist yet — fall back to rename.
    }

    fs::rename(tmp_path, path).context(IoSnafu)
}

/// Serializes and persists the latest tracked values for reuse as SQL parameters.
///
/// Writes to a sibling `.tmp` file and atomically replaces the checkpoint on success.
fn save_params(path: &str, obj: &ObjectMap) -> Result<(), OdbcError> {
    prepare_metadata_path(path)?;
    let path = Path::new(path);
    let tmp_path = checkpoint_temp_path(path);
    let json = serde_json::to_string(obj).context(JsonSnafu)?;

    {
        let mut file = fs::File::create(&tmp_path).context(IoSnafu)?;
        file.write_all(json.as_bytes()).context(IoSnafu)?;
        file.sync_all().context(IoSnafu)?;
    }

    replace_checkpoint_file(&tmp_path, path)
}

/// Localizes a naive datetime with `tz`, using `.latest()` for DST ambiguity and
/// preserving `fallback_text` when the local time does not exist.
fn naive_local_to_timestamp_value(ndt: NaiveDateTime, tz: Tz, fallback_text: &str) -> Value {
    if let Some(dt) = ndt.and_local_timezone(tz).latest() {
        Value::Timestamp(dt.with_timezone(&Utc))
    } else {
        Value::Bytes(Bytes::copy_from_slice(fallback_text.as_bytes()))
    }
}

/// Returns true for SQL-style timestamps with a space date/time separator and offset suffix,
/// such as `YYYY-MM-DD HH:MM:SS+00` or `YYYY-MM-DD HH:MM:SS-05:00`. Returns false for
/// RFC3339 forms that use a `T` separator or a `Z` suffix.
fn sql_timestamp_text_has_offset(text: &str) -> bool {
    if text.len() <= 10 {
        return false;
    }

    // SQL form uses a space between date and time; RFC3339 uses `T`.
    if text.as_bytes().get(10) != Some(&b' ') {
        return false;
    }

    let Some(sign_idx) = text[10..].rfind(['+', '-']) else {
        return false;
    };

    let offset = &text[10 + sign_idx..];
    offset.len() >= 2 && offset[1..].chars().all(|c| c.is_ascii_digit() || c == ':')
}

/// Maps ODBC timestamp bytes to a Vector value.
///
/// SQL offset-bearing forms such as `YYYY-MM-DD HH:MM:SS+00` or `YYYY-MM-DD HH:MM:SS-05:00`
/// are preserved as bytes so tracking parameters round-trip the exact ODBC text. RFC3339/ISO8601
/// values without a SQL-style offset suffix are parsed to `Value::Timestamp`. Naive timestamps
/// are parsed to `Value::Timestamp` using `tz`.
fn map_timestamp_value(value: &[u8], tz: Tz) -> Value {
    let Ok(text) = std::str::from_utf8(value) else {
        return Value::Bytes(Bytes::copy_from_slice(value));
    };

    if sql_timestamp_text_has_offset(text) {
        return Value::Bytes(Bytes::copy_from_slice(value));
    }

    if let Ok(datetime) = DateTime::parse_from_rfc3339(text) {
        return Value::Timestamp(datetime.into());
    }

    TIMESTAMP_FORMATS
        .iter()
        .find_map(|fmt| NaiveDateTime::parse_from_str(text, fmt).ok())
        .map(|ndt| naive_local_to_timestamp_value(ndt, tz, text))
        .unwrap_or_else(|| Value::Bytes(Bytes::copy_from_slice(value)))
}

/// Converts ODBC data types to Vector values.
///
/// # Arguments
/// * `data_type`: The ODBC data type.
/// * `value`: The ODBC value to convert.
/// * `tz`: The timezone to use for date/time conversions.
///
/// # Returns
/// A `Value` compatible with Vector events.
fn map_value(data_type: &odbc_api::DataType, value: Option<&[u8]>, tz: Tz) -> Value {
    match data_type {
        // Convert to bytes.
        odbc_api::DataType::Unknown
        | odbc_api::DataType::Char { .. }
        | odbc_api::DataType::WChar { .. }
        | odbc_api::DataType::Varchar { .. }
        | odbc_api::DataType::WVarchar { .. }
        | odbc_api::DataType::LongVarchar { .. }
        | odbc_api::DataType::WLongVarchar { .. }
        | odbc_api::DataType::Varbinary { .. }
        | odbc_api::DataType::Binary { .. }
        | odbc_api::DataType::Other { .. }
        | odbc_api::DataType::LongVarbinary { .. } => {
            let Some(value) = value else {
                return Value::Null;
            };

            Value::Bytes(Bytes::copy_from_slice(value))
        }

        // Convert to integer.
        odbc_api::DataType::TinyInt
        | odbc_api::DataType::SmallInt
        | odbc_api::DataType::BigInt
        | odbc_api::DataType::Integer => {
            let Some(value) = value else {
                return Value::Null;
            };

            // Preserve unrepresentable integers as bytes so tracking metadata is not lost.
            match std::str::from_utf8(value).map(|s| s.parse::<i64>()) {
                Ok(Ok(i)) => Value::Integer(i),
                _ => Value::Bytes(Bytes::copy_from_slice(value)),
            }
        }

        // Convert to float.
        odbc_api::DataType::Float { .. }
        | odbc_api::DataType::Real
        | odbc_api::DataType::Double => {
            let Some(value) = value else {
                return Value::Null;
            };

            // Preserve unrepresentable floats (for example NaN) as bytes so tracking metadata is not lost.
            // Downstream consumers may see `Value::Bytes` instead of `Value::Float` for NaN and other
            // values that `NotNan` cannot represent.
            match std::str::from_utf8(value).map(NotNan::from_str) {
                Ok(Ok(f)) => Value::Float(f),
                _ => Value::Bytes(Bytes::copy_from_slice(value)),
            }
        }

        // Preserve exact decimal values from the database.
        odbc_api::DataType::Decimal { .. } | odbc_api::DataType::Numeric { .. } => {
            let Some(value) = value else {
                return Value::Null;
            };

            Value::Bytes(Bytes::copy_from_slice(value))
        }

        // Convert to timestamp.
        odbc_api::DataType::Timestamp { .. } => {
            let Some(value) = value else {
                return Value::Null;
            };

            map_timestamp_value(value, tz)
        }

        // Preserve the original time text so tracking parameters bind as `HH:MM:SS`
        // instead of a full timestamp such as `1970-01-01 15:30:00`.
        // MariaDB TIME can represent durations outside a clock-of-day range (for example
        // `25:00:00`), so keep the ODBC text even when chrono cannot parse it.
        odbc_api::DataType::Time { .. } => {
            let Some(value) = value else {
                return Value::Null;
            };

            Value::Bytes(Bytes::copy_from_slice(value))
        }

        // Preserve the original date text so tracking parameters bind as `YYYY-MM-DD`
        // instead of a full timestamp such as `2025-10-04 00:00:00`.
        // MariaDB/MySQL zero dates such as `0000-00-00` are not chrono-compatible but
        // remain valid for SQL comparison and tracking parameter binding.
        odbc_api::DataType::Date => {
            let Some(value) = value else {
                return Value::Null;
            };

            Value::Bytes(Bytes::copy_from_slice(value))
        }

        // Convert to boolean.
        odbc_api::DataType::Bit => {
            let Some(value) = value else {
                return Value::Null;
            };

            Value::Boolean(value[0] == 1 || value[0] == b'1')
        }
    }
}

/// Formats a UTC timestamp as a naive local datetime string for ODBC parameter binding.
fn format_timestamp_for_sql_parameter(timestamp: DateTime<Utc>, tz: Tz) -> String {
    let local = timestamp.with_timezone(&tz);
    if local.nanosecond() != 0 {
        local.format("%Y-%m-%d %H:%M:%S%.f").to_string()
    } else {
        local.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

/// Validates that `map` contains every declared tracking column with a value that can
/// be converted to an ODBC parameter.
pub(crate) fn validate_tracking_state(
    map: &ObjectMap,
    tracking_columns: &[String],
    tz: Tz,
) -> Result<(), String> {
    for column in tracking_columns {
        resolve_tracking_column_parameter(map, column.as_str(), tz)
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

/// Resolves a single tracking column to its source value and the text used for ODBC
/// parameter binding.
fn resolve_tracking_column_parameter(
    map: &ObjectMap,
    column: &str,
    tz: Tz,
) -> Result<(Value, String), OdbcError> {
    let value = map
        .get(column)
        .ok_or_else(|| OdbcError::MissingTrackingColumn {
            column: column.to_owned(),
        })?;
    let param =
        value_to_sql_parameter(value, tz).ok_or_else(|| OdbcError::InvalidTrackingValue {
            column: column.to_owned(),
        })?;
    Ok((value.clone(), param))
}

/// Converts a scalar VRL value to raw text for ODBC parameter binding.
///
/// Unlike `Value::to_string()`, this does not use VRL literal syntax (e.g. quoted
/// strings or `t'…'` timestamps).
///
/// Timestamps are formatted in `tz` as naive local datetimes so they compare
/// consistently with timezone-less database date/time columns.
fn value_to_sql_parameter(value: &Value, tz: Tz) -> Option<String> {
    match value {
        Value::Integer(i) => Some(i.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Boolean(b) => Some(boolean_to_sql_parameter(*b)),
        Value::Bytes(b) => std::str::from_utf8(b).ok().map(str::to_owned),
        Value::Timestamp(t) => Some(format_timestamp_for_sql_parameter(*t, tz)),
        Value::Null => None,
        other => serde_json::to_value(other).ok().and_then(|v| match v {
            serde_json::Value::String(s) => {
                if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(&s) {
                    Some(format_timestamp_for_sql_parameter(datetime.into(), tz))
                } else {
                    Some(s)
                }
            }
            serde_json::Value::Number(n) => Some(n.to_string()),
            serde_json::Value::Bool(b) => Some(boolean_to_sql_parameter(b)),
            _ => None,
        }),
    }
}

/// Formats a boolean as `1`/`0` for ODBC parameter binding.
///
/// Numeric/bit columns (for example MariaDB `BIT`) coerce string parameters to
/// numbers; `"true"` and `"false"` both become `0`, so use bit literals instead.
fn boolean_to_sql_parameter(value: bool) -> String {
    if value { "1" } else { "0" }.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use chrono::TimeZone;

    #[test]
    fn rows_to_events_preserves_typed_values() {
        let timestamp = chrono::Utc
            .with_ymd_and_hms(2025, 10, 4, 12, 34, 56)
            .unwrap();
        let mut row = ObjectMap::new();
        row.insert(KeyString::from("id"), Value::Integer(1));
        row.insert(KeyString::from("active"), Value::Boolean(true));
        row.insert(KeyString::from("datetime_col"), Value::Timestamp(timestamp));
        row.insert(
            KeyString::from("date_col"),
            Value::Bytes(Bytes::from_static(b"2025-10-04")),
        );

        let rows = vec![Value::Object(row)];
        let events = rows_to_events(rows).expect("events");

        assert_eq!(events.len(), 1);

        let Event::Log(log) = &events[0] else {
            panic!("expected log event");
        };

        assert_eq!(log.get("id").unwrap(), &Value::Integer(1));
        assert_eq!(log.get("active").unwrap(), &Value::Boolean(true));
        assert_eq!(
            log.get("datetime_col").unwrap(),
            &Value::Timestamp(timestamp)
        );
        assert_eq!(
            log.get("date_col").unwrap(),
            &Value::Bytes(Bytes::from_static(b"2025-10-04"))
        );
    }

    #[test]
    fn rows_to_events_errors_on_non_object_row() {
        let rows = vec![Value::Integer(1)];
        let error = rows_to_events(rows).expect_err("expected error");
        assert!(matches!(error, OdbcError::InvalidResultRow));
    }

    #[test]
    fn map_value_integer_in_range() {
        let value = map_value(
            &odbc_api::DataType::BigInt,
            Some(b"9223372036854775807"),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Integer(9223372036854775807));
    }

    #[test]
    fn map_value_integer_out_of_range_preserved_as_bytes() {
        let raw = b"18446744073709551615";
        let value = map_value(&odbc_api::DataType::BigInt, Some(raw), chrono_tz::UTC);
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("18446744073709551615".to_owned())
        );
    }

    #[test]
    fn map_value_time_preserved_as_bytes_for_tracking_bind() {
        let raw = b"15:30:00";
        let value = map_value(
            &odbc_api::DataType::Time { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("15:30:00".to_owned())
        );
    }

    #[test]
    fn map_value_date_preserved_as_bytes_for_tracking_bind() {
        let raw = b"2025-10-04";
        let value = map_value(&odbc_api::DataType::Date, Some(raw), chrono_tz::UTC);
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("2025-10-04".to_owned())
        );
    }

    #[test]
    fn map_value_zero_date_preserved_as_bytes_for_tracking_bind() {
        let raw = b"0000-00-00";
        let value = map_value(&odbc_api::DataType::Date, Some(raw), chrono_tz::UTC);
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("0000-00-00".to_owned())
        );
    }

    #[test]
    fn map_value_duration_time_preserved_as_bytes_for_tracking_bind() {
        let raw = b"25:00:00";
        let value = map_value(
            &odbc_api::DataType::Time { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("25:00:00".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_with_offset_preserved_as_bytes_for_tracking_bind() {
        let raw = b"2025-04-28 01:20:04+00";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("2025-04-28 01:20:04+00".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_with_offset_and_colon_preserved_as_bytes_for_tracking_bind() {
        let raw = b"2025-04-28 01:20:04+00:00";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("2025-04-28 01:20:04+00:00".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_with_negative_offset_preserved_as_bytes_for_tracking_bind() {
        let raw = b"2025-04-28 01:20:04-05:00";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("2025-04-28 01:20:04-05:00".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_rfc3339_z_parsed_to_timestamp() {
        let raw = b"2025-04-28T01:20:04Z";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(
            value,
            Value::Timestamp(chrono::Utc.with_ymd_and_hms(2025, 4, 28, 1, 20, 4).unwrap())
        );
    }

    #[test]
    fn map_value_timestamp_rfc3339_t_offset_parsed_to_timestamp() {
        let raw = b"2025-04-28T01:20:04+00:00";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(
            value,
            Value::Timestamp(chrono::Utc.with_ymd_and_hms(2025, 4, 28, 1, 20, 4).unwrap())
        );
    }

    #[test]
    fn map_value_timestamp_unparseable_preserved_as_bytes() {
        let raw = b"not-a-timestamp";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("not-a-timestamp".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_naive_parsed_to_timestamp() {
        let raw = b"2025-10-04 12:34:56";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(
            value,
            Value::Timestamp(
                chrono::Utc
                    .with_ymd_and_hms(2025, 10, 4, 12, 34, 56)
                    .unwrap()
            )
        );
    }

    #[test]
    fn order_params_follows_tracking_column_order() {
        let mut map = ObjectMap::new();
        map.insert(KeyString::from("id"), Value::Integer(1));
        map.insert(KeyString::from("name"), Value::from("vector"));
        let columns = vec!["id".to_owned(), "name".to_owned()];
        let expected: Vec<String> = columns
            .iter()
            .map(|column| {
                value_to_sql_parameter(map.get(column.as_str()).unwrap(), chrono_tz::UTC).unwrap()
            })
            .collect();

        let params = order_params(&map, Some(&columns), chrono_tz::UTC).expect("params");

        assert_eq!(expected, vec!["1", "vector"]);
        assert_eq!(params.len(), expected.len());
    }

    #[test]
    fn order_params_errors_on_missing_tracking_column() {
        let mut map = ObjectMap::new();
        map.insert(KeyString::from("id"), Value::Integer(1));

        let error = match order_params(
            &map,
            Some(&vec!["id".to_owned(), "name".to_owned()]),
            chrono_tz::UTC,
        ) {
            Err(error) => error,
            Ok(_) => panic!("expected missing tracking column error"),
        };

        assert!(matches!(
            error,
            OdbcError::MissingTrackingColumn { column } if column == "name"
        ));
    }

    #[test]
    fn order_params_errors_on_unconvertible_tracking_value() {
        let mut map = ObjectMap::new();
        map.insert(KeyString::from("id"), Value::Null);

        let error = match order_params(&map, Some(&vec!["id".to_owned()]), chrono_tz::UTC) {
            Err(error) => error,
            Ok(_) => panic!("expected invalid tracking value error"),
        };

        assert!(matches!(
            error,
            OdbcError::InvalidTrackingValue { column } if column == "id"
        ));
    }

    #[tokio::test]
    async fn extract_and_save_tracking_errors_on_missing_column() {
        let mut obj = ObjectMap::new();
        obj.insert(KeyString::from("id"), Value::Integer(1));

        let error = match extract_and_save_tracking(
            None,
            Value::Object(obj),
            vec!["id".to_owned(), "name".to_owned()],
            chrono_tz::UTC,
        )
        .await
        {
            Err(error) => error,
            Ok(_) => panic!("expected missing tracking column error"),
        };

        assert!(matches!(
            error,
            OdbcError::MissingTrackingColumn { column } if column == "name"
        ));
    }

    #[tokio::test]
    async fn extract_and_save_tracking_errors_on_invalid_tracking_row() {
        let error = match extract_and_save_tracking(
            None,
            Value::Integer(1),
            vec!["id".to_owned()],
            chrono_tz::UTC,
        )
        .await
        {
            Err(error) => error,
            Ok(_) => panic!("expected invalid tracking row error"),
        };

        assert!(matches!(error, OdbcError::InvalidTrackingRow));
    }

    #[tokio::test]
    async fn extract_and_save_tracking_persists_declared_columns() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let path = temp_dir.path().join("tracking.json");
        let path = path.to_str().expect("utf-8 path");
        let mut obj = ObjectMap::new();
        obj.insert(KeyString::from("id"), Value::Integer(42));
        obj.insert(KeyString::from("name"), Value::from("vector"));

        let saved = extract_and_save_tracking(
            Some(path),
            Value::Object(obj),
            vec!["id".to_owned(), "name".to_owned()],
            chrono_tz::UTC,
        )
        .await
        .expect("saved tracking state")
        .expect("tracking object");

        assert_eq!(saved.len(), 2);
        assert_eq!(
            saved.get("id"),
            Some(&Value::Bytes(Bytes::from_static(b"42")))
        );
        assert_eq!(
            saved.get("name"),
            Some(&Value::Bytes(Bytes::from_static(b"vector")))
        );
        assert!(std::path::Path::new(path).exists());
    }

    #[test]
    fn load_params_reads_valid_tracking_metadata() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let path = temp_dir.path().join("tracking.json");
        fs::write(&path, r#"{"id":1,"name":"vector"}"#).expect("write metadata");
        let path = path.to_str().expect("utf-8 path");
        let columns = vec!["id".to_owned(), "name".to_owned()];

        let params = load_params(path, Some(&columns), chrono_tz::UTC)
            .expect("load params")
            .expect("metadata params");

        assert_eq!(params.len(), 2);
    }

    #[test]
    fn validate_tracking_state_errors_on_missing_column() {
        let mut map = ObjectMap::new();
        map.insert(KeyString::from("id"), Value::Integer(1));

        let error =
            validate_tracking_state(&map, &["id".to_owned(), "name".to_owned()], chrono_tz::UTC)
                .expect_err("validation error");

        assert!(error.contains("name"));
    }

    #[test]
    fn prepare_metadata_path_creates_parent_directory() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let path = temp_dir.path().join("nested").join("tracking.json");
        let path = path.to_str().expect("utf-8 path");

        prepare_metadata_path(path).expect("prepare metadata path");

        assert!(temp_dir.path().join("nested").is_dir());
    }

    #[test]
    fn prepare_metadata_path_rejects_empty_path() {
        let error = match prepare_metadata_path("") {
            Err(error) => error,
            Ok(_) => panic!("expected empty path error"),
        };

        assert!(matches!(error, OdbcError::Io { .. }));
    }

    #[test]
    fn ensure_unique_column_names_accepts_distinct_names() {
        ensure_unique_column_names(&["id".to_owned(), "name".to_owned()])
            .expect("distinct column names");
    }

    #[test]
    fn ensure_unique_column_names_errors_on_duplicates() {
        let error = match ensure_unique_column_names(&[
            "id".to_owned(),
            "name".to_owned(),
            "id".to_owned(),
        ]) {
            Err(error) => error,
            Ok(_) => panic!("expected duplicate column names error"),
        };

        assert!(matches!(
            error,
            OdbcError::DuplicateColumnNames { columns } if columns == vec!["id".to_owned()]
        ));
    }

    #[test]
    fn save_params_overwrites_existing_checkpoint() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let path = temp_dir.path().join("tracking.json");
        let path = path.to_str().expect("utf-8 path");

        let mut first = ObjectMap::new();
        first.insert(KeyString::from("id"), Value::Integer(1));
        save_params(path, &first).expect("first save");

        let mut second = ObjectMap::new();
        second.insert(KeyString::from("id"), Value::Integer(2));
        save_params(path, &second).expect("second save should overwrite");

        let saved: ObjectMap =
            serde_json::from_reader(File::open(path).expect("open checkpoint")).expect("parse");
        assert_eq!(saved.get("id"), Some(&Value::Integer(2)));
    }

    #[test]
    fn ensure_unique_column_names_errors_on_multiple_duplicates() {
        let error = match ensure_unique_column_names(&[
            "id".to_owned(),
            "name".to_owned(),
            "id".to_owned(),
            "name".to_owned(),
        ]) {
            Err(error) => error,
            Ok(_) => panic!("expected duplicate column names error"),
        };

        assert!(matches!(
            error,
            OdbcError::DuplicateColumnNames { columns }
                if columns == vec!["id".to_owned(), "name".to_owned()]
        ));
    }
}
