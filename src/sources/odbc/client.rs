use crate::config::{SourceContext, log_schema};
use crate::internal_events::{OdbcEventsReceived, OdbcFailedError, OdbcQueryExecuted};
use crate::sinks::prelude::*;
use crate::sources::odbc::config::OdbcConfig;
use crate::sources::odbc::{
    ClosedSnafu, Column, Columns, ConfigSnafu, DbSnafu, OdbcError, Rows, load_params, map_value,
    order_params, save_params,
};
use chrono::Utc;
use chrono_tz::Tz;
use futures::pin_mut;
use futures_util::StreamExt;
use itertools::Itertools;
use odbc_api::buffers::TextRowSet;
use odbc_api::parameter::VarCharBox;
use odbc_api::{ConnectionOptions, Cursor, Environment, ResultSetMetadata};
use snafu::{OptionExt, ResultExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::select;
use vector_common::internal_event::{BytesReceived, Protocol};
use vector_lib::emit;
use vrl::prelude::*;

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
        out.send_event(event).await.context(ClosedSnafu)?;

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
