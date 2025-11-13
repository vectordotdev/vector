use crate::config::{LogNamespace, SourceConfig, SourceContext, SourceOutput, log_schema};
use crate::serde::default_decoding;
use crate::sources::Source;
use crate::sources::odbc::OdbcSchedule;
use crate::sources::odbc::client::Context;
use chrono_tz::Tz;
use futures_util::FutureExt;
use serde_with::DurationSeconds;
use serde_with::serde_as;
use std::fs;
use std::time::Duration;
use vector_config_macros::configurable_component;
use vector_lib::codecs::decoding::DeserializerConfig;
use vrl::prelude::{Kind, ObjectMap};

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

const fn default_query_timeout_sec() -> Duration {
    Duration::from_secs(3)
}

const fn default_schedule_timezone() -> Tz {
    Tz::UTC
}

const fn default_odbc_default_timezone() -> Tz {
    default_schedule_timezone()
}

const fn default_odbc_batch_size() -> usize {
    100
}

const fn default_odbc_max_str_limit() -> usize {
    4096
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
