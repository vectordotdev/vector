use crate::config::{SourceContext, log_schema};
use crate::internal_events::{OdbcEventsReceived, OdbcFailedError, OdbcQueryExecuted};
use crate::sinks::prelude::*;
use crate::sources::odbc::config::OdbcConfig;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use futures::pin_mut;
use futures_util::StreamExt;
use itertools::Itertools;
use odbc_api::buffers::TextRowSet;
use odbc_api::parameter::VarCharBox;
use odbc_api::{ConnectionOptions, Cursor, Environment, IntoParameter, ResultSetMetadata};
use snafu::{OptionExt, ResultExt, Snafu};
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::select;
use vector_common::internal_event::{BytesReceived, Protocol};
use vector_lib::emit;
use vector_lib::source_sender::SendError;
use vrl::prelude::*;

const TIMESTAMP_FORMATS: &[&str] = &[
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%dT%H:%M:%S",
    "%Y/%m/%d %H:%M:%S",
    "%Y/%m/%dT%H:%M:%S",
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

    #[snafu(display("Configuration error: {cause}"))]
    ConfigError { cause: &'static str },
}

pub(crate) struct Context {
    cfg: OdbcConfig,
    env: Arc<Environment>,
    cx: SourceContext,
}

impl Context {
    pub(crate) fn new(cfg: OdbcConfig, cx: SourceContext) -> Result<Self, OdbcError> {
        let env = Environment::new().context(DbSnafu)?;

        Ok(Self {
            cfg,
            env: Arc::new(env),
            cx,
        })
    }

    pub(crate) async fn run_schedule(self: Box<Self>) -> Result<(), ()> {
        let shutdown = self.cx.shutdown.clone();

        let Some(ref schedule) = self.cfg.schedule else {
            warn!(message = "No next schedule found. Retry in 10 seconds.");
            return Err(());
        };

        let schedule = schedule.clone().stream(self.cfg.schedule_timezone);
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
                    emit!(OdbcEventsReceived {
                        count: 1,
                    });

                    let instant = Instant::now();
                    if let Ok(result) = self.process(prev_result.clone()).await {

                        // Update the cached result when the query returns rows.
                        if result.is_some() {
                            prev_result = result;
                        }

                        emit!(OdbcQueryExecuted {
                          statement: &self.cfg.statement.clone().unwrap_or_default(),
                          elapsed: instant.elapsed().as_millis()
                        })
                    } else {
                        emit!(OdbcFailedError {
                            statement: &self.cfg.statement.clone().unwrap_or_default(),
                        })
                    }

                    // When no further schedule is defined, run once and then stop.
                    if next.is_none() {
                        debug!(message = "No additional schedule configured. Shutting down ODBC source.");
                        break
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
        let conn_str = self.cfg.connection_string_or_file();
        let stmt_str = self.cfg.statement_or_file().context(ConfigSnafu {
            cause: "No statement",
        })?;
        let out = self.cx.out.clone();
        let log_schema = log_schema();
        let env = Arc::clone(&self.env);

        // Load the last-run metadata from disk when available.
        // If the file is missing, fall back to the initial parameters or the latest query result.
        let stmt_params = self
            .cfg
            .last_run_metadata_path
            .as_ref()
            .and_then(|path| load_params(path, self.cfg.tracking_columns.as_ref()))
            .unwrap_or(
                order_params(map.unwrap_or_default(), self.cfg.tracking_columns.as_ref())
                    .unwrap_or_default(),
            );
        let cfg = self.cfg.clone();

        let rows = execute_query(
            &env,
            &conn_str,
            &stmt_str,
            stmt_params,
            cfg.statement_timeout,
            cfg.odbc_default_timezone,
            cfg.odbc_batch_size,
            cfg.odbc_max_str_limit,
        )?;

        // Example with query results: `{"message":[{ ... }],"timestamp":"2025-10-21T00:00:00.05275Z"}`
        // Example with no query results: `{"message":[],"timestamp":"2025-10-21T00:00:00.05275Z"}`
        let mut event = LogEvent::default();
        event.maybe_insert(Some("timestamp"), Value::Timestamp(Utc::now()));
        event.maybe_insert(
            log_schema.message_key_target_path(),
            Value::Array(rows.clone()),
        );

        let mut out = out.clone();
        out.send_event(event).await.context(SendSnafu)?;

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
fn load_params(path: &str, columns_order: Option<&Vec<String>>) -> Option<Vec<VarCharBox>> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let map: ObjectMap = serde_json::from_reader(reader).ok()?;

    order_params(map, columns_order)
}

/// Orders the parameters of a given `ObjectMap` based on an optional column order.
fn order_params(map: ObjectMap, columns_order: Option<&Vec<String>>) -> Option<Vec<VarCharBox>> {
    if columns_order.is_none() || columns_order.iter().len() == 0 {
        let params = map
            .iter()
            .map(|p| p.1.to_string().into_parameter())
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
            Some(value.to_string().into_parameter())
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

            // tinyint(1) -> Value::Boolean
            if *data_type == odbc_api::DataType::TinyInt
                && value.len() == 1
                && (value[0] == b'0' || value[0] == b'1')
            {
                return Value::Boolean(value[0] == b'1');
            }

            std::str::from_utf8(value)
                .ok()
                .and_then(|s| s.parse::<i64>().ok())
                .map_or(Value::Null, Value::Integer)
        }

        // Convert to float.
        odbc_api::DataType::Float { .. }
        | odbc_api::DataType::Real
        | odbc_api::DataType::Decimal { .. }
        | odbc_api::DataType::Numeric { .. }
        | odbc_api::DataType::Double => {
            let Some(value) = value else {
                return Value::Null;
            };

            std::str::from_utf8(value)
                .ok()
                .and_then(|s| NotNan::from_str(s).ok())
                .map_or(Value::Null, Value::Float)
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
                .map(|ndt| ndt.and_utc());

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
                .map(|time| {
                    let datetime = NaiveDateTime::new(NaiveDate::default(), time);
                    let tz = tz.offset_from_utc_datetime(&datetime);
                    Value::Timestamp(
                        DateTime::<Tz>::from_naive_utc_and_offset(datetime, tz).to_utc(),
                    )
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
                .map(|date| {
                    let datetime = NaiveDateTime::new(date, NaiveTime::default());
                    let tz = tz.offset_from_utc_datetime(&datetime);
                    Value::Timestamp(
                        DateTime::<Tz>::from_naive_utc_and_offset(datetime, tz).to_utc(),
                    )
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
