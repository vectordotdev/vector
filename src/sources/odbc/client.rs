use crate::config::{LogNamespace, SourceConfig, SourceContext, SourceOutput, log_schema};
use crate::internal_events::{OdbcEventsReceived, OdbcFailedError, OdbcQueryExecuted};
use crate::serde::default_decoding;
use crate::sinks::prelude::*;
use crate::sources::Source;
use crate::sources::odbc::{
    ClosedSnafu, Column, Columns, ConfigSnafu, DbSnafu, OdbcError, OdbcSchedule, Rows, load_params,
    map_value, order_params, save_params,
};
use chrono::Utc;
use chrono_tz::Tz;
use futures::pin_mut;
use futures_util::StreamExt;
use itertools::Itertools;
use odbc_api::buffers::TextRowSet;
use odbc_api::parameter::VarCharBox;
use odbc_api::{ConnectionOptions, Cursor, Environment, ResultSetMetadata};
use serde_with::DurationSeconds;
use serde_with::serde_as;
use snafu::{OptionExt, ResultExt};
use std::fmt::Debug;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::select;
use typetag::serde;
use vector_common::internal_event::{BytesReceived, Protocol};
use vector_lib::codecs::decoding::DeserializerConfig;
use vector_lib::emit;
use vrl::prelude::*;

/// Configuration for the `odbc` source.
#[serde_as]
#[configurable_component(source(
    "odbc",
    "Periodically pulls observability data from an ODBC interface by running a scheduled query."
))]
#[derive(Clone, Debug)]
pub struct OdbcConfig {
    /// The connection string to use for ODBC.
    /// If the `connection_string_filepath` is set, this value is ignored.
    #[configurable(metadata(
        docs::examples = "driver={MariaDB Unicode};server=<ip or host>;port=<port number>;database=<database name>;uid=<user>;pwd=<password>"
    ))]
    pub connection_string: String,

    /// The path to the file that contains the connection string.
    /// If this is not set or the file at that path does not exist, the `connection_string` field is used instead.
    #[configurable(metadata(
        docs::examples = "driver={MariaDB Unicode};server=<ip or host>;port=<port number>;database=<database name>;uid=<user>;pwd=<password>"
    ))]
    pub connection_string_filepath: Option<String>,

    /// The SQL statement to execute.
    /// This SQL statement is executed periodically according to the `schedule`.
    /// Defaults to `None`. If no SQL statement is provided, the source returns an error.
    /// If the `statement_filepath` is set, this value is ignored.
    #[configurable(metadata(docs::examples = "SELECT * FROM users WHERE id = ?"))]
    pub statement: Option<String>,

    /// The path to the file that contains the SQL statement.
    /// If this is unset or the file cannot be read, the value from `statement` is used instead.
    pub statement_filepath: Option<String>,

    /// Maximum time to allow the SQL statement to run.
    /// If the query does not finish within this window, it is canceled and retried at the next scheduled run.
    /// The default is 3 seconds.
    #[configurable(metadata(docs::examples = 3))]
    #[configurable(metadata(
        docs::additional_props_description = "Maximum time to wait for the SQL statement to execute"
    ))]
    #[serde(default = "default_query_timeout_sec")]
    #[serde_as(as = "DurationSeconds<u64>")]
    pub statement_timeout: Duration,

    /// Initial parameters for the first execution of the statement.
    /// Used if `last_run_metadata_path` does not exist.
    /// Values must be strings and follow the parameter order defined in the query.
    ///
    /// # Examples
    ///
    /// When the source runs for the first time, the file at `last_run_metadata_path` does not exist.
    /// In that case, declare the initial values in `statement_init_params`.
    ///
    /// ```toml
    /// [sources.odbc]
    /// statement = "SELECT * FROM users WHERE id = ?"
    /// statement_init_params = { "id": "0" }
    /// tracking_columns = ["id"]
    /// last_run_metadata_path = "/path/to/tracking.json"
    /// # The rest of the fields are omitted
    /// ```
    #[configurable(metadata(
        docs::additional_props_description = "Initial value for the SQL statement parameters. The value is always a string."
    ))]
    pub statement_init_params: Option<ObjectMap>,

    /// Cron expression used to schedule database queries.
    /// When omitted, the statement runs only once by default.
    #[configurable(derived)]
    pub schedule: Option<OdbcSchedule>,

    /// The timezone to use for the `schedule`.
    /// Typically the timezone used when evaluating the cron expression.
    /// The default is UTC.
    ///
    /// [Wikipedia]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
    #[configurable(metadata(docs::examples = "UTC"))]
    #[configurable(metadata(
        docs::additional_props_description = "Timezone to use for the schedule"
    ))]
    #[serde(default = "default_schedule_timezone")]
    pub schedule_timezone: Tz,

    /// Number of rows to fetch per batch from the ODBC driver.
    /// The default is 100.
    #[configurable(metadata(docs::examples = 100))]
    #[serde(default = "default_odbc_batch_size")]
    pub odbc_batch_size: usize,

    /// Maximum string length for ODBC driver operations.
    /// The default is 4096.
    #[configurable(metadata(docs::examples = 4096))]
    #[serde(default = "default_odbc_batch_size")]
    pub odbc_max_str_limit: usize,

    /// Timezone applied to database date/time columns that lack timezone information.
    /// The default is UTC.
    #[configurable(metadata(docs::examples = "UTC"))]
    #[configurable(metadata(
        docs::additional_props_description = "Timezone to use for the database date/time type without a timezone"
    ))]
    #[serde(default = "default_odbc_default_timezone")]
    pub odbc_default_timezone: Tz,

    /// Specifies the columns to track from the last row of the statement result set.
    /// Their values are passed as parameters to the SQL statement in the next scheduled run.
    ///
    /// # Examples
    ///
    /// ```toml
    /// [sources.odbc]
    /// statement = "SELECT * FROM users WHERE id = ?"
    /// tracking_columns = ["id"]
    /// # The rest of the fields are omitted
    /// ```
    #[configurable(metadata(docs::examples = "id"))]
    pub tracking_columns: Option<Vec<String>>,

    /// The path to the file where the last row of the result set will be saved.
    /// The last row of the result set is saved in JSON format.
    /// This file provides parameters for the SQL query in the next scheduled run.
    /// If the file does not exist or the path is not specified, the initial value from `statement_init_params` is used.
    ///
    /// # Examples
    ///
    /// If `tracking_columns = ["id", "name"]`, it is saved as the following JSON data.
    ///
    /// ```json
    /// {"id":1, "name": "vector"}
    /// ```
    #[configurable(metadata(docs::examples = "/path/to/tracking.json"))]
    pub last_run_metadata_path: Option<String>,

    /// Decoder to use for query results.
    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

    #[cfg(test)]
    #[configurable(derived)]
    #[serde(default)]
    pub iterations: Option<usize>,
}

const fn default_query_timeout_sec() -> Duration {
    Duration::from_secs(3)
}

const fn default_schedule_timezone() -> Tz {
    Tz::UTC
}

const fn default_odbc_batch_size() -> usize {
    100
}

const fn default_odbc_max_str_limit() -> usize {
    4096
}

const fn default_odbc_default_timezone() -> Tz {
    default_schedule_timezone()
}

impl Default for OdbcConfig {
    fn default() -> Self {
        Self {
            connection_string: "".to_string(),
            connection_string_filepath: None,
            schedule: None,
            schedule_timezone: Tz::UTC,
            statement: None,
            statement_timeout: Duration::from_secs(3),
            statement_init_params: None,
            odbc_batch_size: default_odbc_batch_size(),
            odbc_max_str_limit: default_odbc_max_str_limit(),
            odbc_default_timezone: Tz::UTC,
            tracking_columns: None,
            last_run_metadata_path: None,
            decoding: default_decoding(),
            log_namespace: None,
            statement_filepath: None,
            #[cfg(test)]
            iterations: None,
        }
    }
}

impl OdbcConfig {
    /// Returns the connection string to use for ODBC.
    /// If the `connection_string_filepath` is set, read the file and return its content.
    pub fn connection_string_or_file(&self) -> String {
        self.connection_string_filepath
            .as_ref()
            .and_then(|path| fs::read_to_string(path).ok())
            .unwrap_or(self.connection_string.clone())
    }

    /// Returns the SQL statement to execute.
    /// If the `statement_filepath` is set, read the file and return its content.
    pub fn statement_or_file(&self) -> Option<String> {
        self.statement_filepath
            .as_ref()
            .map(|path| fs::read_to_string(path).ok())
            .unwrap_or(self.statement.clone())
    }
}

impl_generate_config_from_default!(OdbcConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "odbc")]
impl SourceConfig for OdbcConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let guard = Context::new(self.clone(), cx)?;
        let context = Box::new(guard);
        Ok(context.run_schedule().boxed())
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let mut schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        if let Some(timestamp_key) = log_schema().timestamp_key() {
            schema_definition = schema_definition.optional_field(
                timestamp_key,
                Kind::timestamp(),
                Some("timestamp"),
            )
        }

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

struct Context {
    cfg: OdbcConfig,
    env: Arc<Environment>,
    cx: SourceContext,
}

impl Context {
    fn new(cfg: OdbcConfig, cx: SourceContext) -> Result<Self, OdbcError> {
        let env = Environment::new().context(DbSnafu)?;

        Ok(Self {
            cfg,
            env: Arc::new(env),
            cx,
        })
    }

    async fn run_schedule(self: Box<Self>) -> Result<(), ()> {
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
