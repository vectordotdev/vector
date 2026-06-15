use crate::config::{LogNamespace, SourceContext, log_schema};
use crate::event::Event;
use crate::internal_events::{OdbcEventsReceived, OdbcFailedError, OdbcQueryExecuted};
use crate::sinks::prelude::*;
use crate::sources::odbc::config::OdbcConfig;
use bytes::BytesMut;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Utc};
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
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio::select;
use tokio_util::codec::Decoder as _;
use vector_common::internal_event::{BytesReceived, Protocol, error_stage, error_type};
use vector_lib::EstimatedJsonEncodedSizeOf;
use vector_lib::codecs::Decoder;
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

    #[snafu(display("Decode error: {source}"))]
    Decode {
        source: vector_lib::codecs::decoding::Error,
    },

    #[snafu(display("Blocking ODBC task failed: {source}"))]
    BlockingTask { source: tokio::task::JoinError },
}

impl OdbcError {
    pub(crate) const fn error_type(&self) -> &'static str {
        match self {
            Self::Db { .. } => error_type::REQUEST_FAILED,
            Self::Io { .. } => error_type::IO_FAILED,
            Self::SendError { .. } => error_type::WRITER_FAILED,
            Self::Json { .. } | Self::Decode { .. } => error_type::PARSER_FAILED,
            Self::BlockingTask { .. } => error_type::REQUEST_FAILED,
        }
    }

    pub(crate) const fn error_stage(&self) -> &'static str {
        match self {
            Self::SendError { .. } => error_stage::SENDING,
            Self::Json { .. } | Self::Decode { .. } => error_stage::PROCESSING,
            _ => error_stage::RECEIVING,
        }
    }
}

pub(crate) struct Context {
    cfg: OdbcConfig,
    env: &'static Environment,
    cx: SourceContext,
    decoder: Decoder,
    log_namespace: LogNamespace,
}

impl Context {
    pub(crate) fn new(
        cfg: OdbcConfig,
        cx: SourceContext,
        decoder: Decoder,
        log_namespace: LogNamespace,
    ) -> Result<Self, OdbcError> {
        let env = environment().context(DbSnafu)?;

        Ok(Self {
            cfg,
            env,
            cx,
            decoder,
            log_namespace,
        })
    }

    pub(crate) async fn run_schedule(self: Box<Self>) -> Result<(), ()> {
        let shutdown = self.cx.shutdown.clone();

        let schedule = self.cfg.schedule.clone().stream(self.cfg.schedule_timezone);
        pin_mut!(schedule);

        let _ = register!(BytesReceived::from(Protocol::NONE));

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
                    match self.process(prev_result.clone()).await {
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

    /// Executes the scheduled ODBC query, sends the result as an event, and updates tracking metadata.
    async fn process(&self, map: Option<ObjectMap>) -> Result<Option<ObjectMap>, OdbcError> {
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
        let stmt_params = if let Some(path) = &self.cfg.last_run_metadata_path {
            match load_params(path, self.cfg.tracking_columns.as_ref())? {
                Some(params) => params,
                None => order_params(map.unwrap_or_default(), self.cfg.tracking_columns.as_ref())
                    .unwrap_or_default(),
            }
        } else {
            order_params(map.unwrap_or_default(), self.cfg.tracking_columns.as_ref())
                .unwrap_or_default()
        };
        let cfg = self.cfg.clone();
        let timeout = cfg.statement_timeout;
        let tz = cfg.odbc_default_timezone;
        let batch_size = cfg.odbc_batch_size;
        let max_str_limit = cfg.odbc_max_str_limit;

        let rows = tokio::task::spawn_blocking(move || {
            execute_query(
                env,
                &conn_str,
                &stmt_str,
                stmt_params,
                timeout,
                tz,
                batch_size,
                max_str_limit,
            )
        })
        .await
        .context(BlockingTaskSnafu)??;

        let mut events = self.decode_rows(&rows)?;
        self.enrich_events(&mut events);

        let event_count = events.len();
        if event_count > 0 {
            let byte_size = events.estimated_json_encoded_size_of();
            let mut out = out.clone();
            out.send_batch(events).await.context(SendSnafu)?;
            emit!(OdbcEventsReceived {
                count: event_count,
                byte_size,
            });
        }

        if let Some(last) = rows.last() {
            let Some(tracking_columns) = cfg.tracking_columns else {
                return Ok(None);
            };
            let latest_result = extract_and_save_tracking(
                cfg.last_run_metadata_path.as_deref(),
                last.clone(),
                tracking_columns,
            )
            .await?;
            return Ok(latest_result);
        }

        Ok(None)
    }

    fn decode_rows(&self, rows: &Rows) -> Result<Vec<Event>, OdbcError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let payload = serde_json::to_vec(rows).context(JsonSnafu)?;
        let mut buf = BytesMut::from(payload.as_slice());
        let mut events = Vec::new();
        let mut decoder = self.decoder.clone();

        loop {
            match decoder.decode_eof(&mut buf) {
                Ok(Some((next, _))) => events.extend(next),
                Ok(None) => break,
                Err(error) => {
                    // tracking metadata is not advanced past rows that were not decoded.
                    return Err(OdbcError::Decode { source: error });
                }
            }
        }

        Ok(events)
    }

    fn enrich_events(&self, events: &mut [Event]) {
        let now = Utc::now();

        for event in events {
            let Event::Log(log) = event else {
                continue;
            };

            self.log_namespace
                .insert_standard_vector_source_metadata(log, OdbcConfig::NAME, now);

            if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
                log.try_insert(timestamp_key, now);
            }
        }
    }
}

/// Extracts specified tracking columns from the given object,
/// saves them to a file if a path is provided.
async fn extract_and_save_tracking(
    path: Option<&str>,
    obj: Value,
    tracking_columns: Vec<String>,
) -> Result<Option<ObjectMap>, OdbcError> {
    let tracking_columns = tracking_columns
        .iter()
        .map(|col| col.as_str())
        .collect_vec();

    if let Value::Object(obj) = obj {
        let save_obj = obj
            .iter()
            .filter(|item| tracking_columns.contains(&item.0.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if let Some(path) = path {
            save_params(path, &save_obj)?;
        }
        return Ok(Some(save_obj));
    }

    Ok(None)
}

/// Executes an ODBC SQL query with optional parameters, fetches rows in batches,
/// and returns the results as a vector of objects.
#[allow(clippy::too_many_arguments)]
pub fn execute_query(
    env: &Environment,
    conn_str: &str,
    stmt_str: &str,
    stmt_params: Vec<VarCharBox>,
    timeout: Duration,
    tz: Tz,
    batch_size: usize,
    max_str_limit: usize,
) -> Result<Rows, OdbcError> {
    let conn = env
        .connect_with_connection_string(conn_str, ConnectionOptions::default())
        .context(DbSnafu)?;
    let mut statement = conn.preallocate().context(DbSnafu)?;
    statement
        .set_query_timeout_sec(timeout.as_secs() as usize)
        .context(DbSnafu)?;

    let result = if stmt_params.is_empty() {
        statement.execute(stmt_str, ())
    } else {
        statement.execute(stmt_str, &stmt_params[..])
    }
    .context(DbSnafu)?;

    let Some(mut cursor) = result else {
        return Ok(Rows::default());
    };

    let names = cursor
        .column_names()
        .context(DbSnafu)?
        .collect::<Result<Vec<String>, _>>()
        .context(DbSnafu)?;
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

    let buffer =
        TextRowSet::for_cursor(batch_size, &mut cursor, Some(max_str_limit)).context(DbSnafu)?;
    let mut row_set_cursor = cursor.bind_buffer(buffer).context(DbSnafu)?;
    let mut rows = Rows::with_capacity(batch_size);

    while let Some(batch) = row_set_cursor.fetch().context(DbSnafu)? {
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

            rows.push(Value::Object(cols))
        }
    }

    Ok(rows)
}

/// Loads the previously saved result and returns it as SQL parameters.
/// Parameters are generated in the order specified by `columns_order`.
///
/// Returns `Ok(None)` only when the metadata file does not exist.
fn load_params(
    path: &str,
    columns_order: Option<&Vec<String>>,
) -> Result<Option<Vec<VarCharBox>>, OdbcError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(OdbcError::Io { source }),
    };
    let reader = BufReader::new(file);
    let map: ObjectMap = serde_json::from_reader(reader).context(JsonSnafu)?;

    Ok(order_params(map, columns_order))
}

/// Orders the parameters of a given `ObjectMap` based on an optional column order.
fn order_params(map: ObjectMap, columns_order: Option<&Vec<String>>) -> Option<Vec<VarCharBox>> {
    if columns_order.is_none() || columns_order.iter().len() == 0 {
        let params = map
            .iter()
            .filter_map(|(_, value)| {
                value_to_sql_parameter(value).map(|param| param.into_parameter())
            })
            .collect_vec();
        return Some(params);
    }

    let binding = vec![];
    let columns_order = columns_order
        .unwrap_or(&binding)
        .iter()
        .map(|col| col.as_str())
        .collect_vec();

    // Ensure parameters follow the declared column order.
    let params = columns_order
        .into_iter()
        .filter_map(|col| {
            let value = map.get(col)?;
            value_to_sql_parameter(value).map(|param| param.into_parameter())
        })
        .collect_vec();

    Some(params)
}

/// Serializes and persists the latest tracked values for reuse as SQL parameters.
fn save_params(path: &str, obj: &ObjectMap) -> Result<(), OdbcError> {
    let json = serde_json::to_string(obj).context(JsonSnafu)?;
    fs::write(path, json).context(IoSnafu)
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

            std::str::from_utf8(value)
                .ok()
                .and_then(|s| s.parse::<i64>().ok())
                .map_or(Value::Null, Value::Integer)
        }

        // Convert to float.
        odbc_api::DataType::Float { .. }
        | odbc_api::DataType::Real
        | odbc_api::DataType::Double => {
            let Some(value) = value else {
                return Value::Null;
            };

            std::str::from_utf8(value)
                .ok()
                .and_then(|s| NotNan::from_str(s).ok())
                .map_or(Value::Null, Value::Float)
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

            let Ok(str) = std::str::from_utf8(value) else {
                return Value::Null;
            };

            // Try RFC3339/ISO8601 first and convert to UTC
            if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(str) {
                return Value::Timestamp(datetime.into());
            }

            let datetime = TIMESTAMP_FORMATS
                .iter()
                .find_map(|fmt| NaiveDateTime::parse_from_str(str, fmt).ok())
                .and_then(|ndt| ndt.and_local_timezone(tz).single())
                .map(|dt| dt.with_timezone(&Utc));

            datetime.map(Value::Timestamp).unwrap_or(Value::Null)
        }

        // Convert to timestamp.
        odbc_api::DataType::Time { .. } => {
            let Some(value) = value else {
                return Value::Null;
            };

            std::str::from_utf8(value)
                .ok()
                .and_then(|s| NaiveTime::from_str(s).ok())
                .and_then(|time| {
                    NaiveDateTime::new(NaiveDate::default(), time)
                        .and_local_timezone(tz)
                        .single()
                        .map(|dt| Value::Timestamp(dt.with_timezone(&Utc)))
                })
                .unwrap_or(Value::Null)
        }

        // Convert to timestamp.
        odbc_api::DataType::Date => {
            let Some(value) = value else {
                return Value::Null;
            };

            std::str::from_utf8(value)
                .ok()
                .and_then(|s| chrono::NaiveDate::from_str(s).ok())
                .and_then(|date| {
                    NaiveDateTime::new(date, NaiveTime::default())
                        .and_local_timezone(tz)
                        .single()
                        .map(|dt| Value::Timestamp(dt.with_timezone(&Utc)))
                })
                .unwrap_or(Value::Null)
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

/// Converts a scalar VRL value to raw text for ODBC parameter binding.
///
/// Unlike `Value::to_string()`, this does not use VRL literal syntax (e.g. quoted
/// strings or `t'…'` timestamps).
fn value_to_sql_parameter(value: &Value) -> Option<String> {
    match value {
        Value::Integer(i) => Some(i.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Boolean(b) => Some(b.to_string()),
        Value::Bytes(b) => std::str::from_utf8(b).ok().map(str::to_owned),
        Value::Timestamp(t) => Some(t.to_rfc3339()),
        Value::Null => None,
        other => serde_json::to_value(other).ok().and_then(|v| match v {
            serde_json::Value::String(s) => Some(s),
            serde_json::Value::Number(n) => Some(n.to_string()),
            serde_json::Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }),
    }
}
