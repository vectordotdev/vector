use crate::config::{LogNamespace, SourceConfig, SourceContext, SourceOutput, log_schema};
use crate::sources::Source;
use crate::sources::odbc::client::{Context, prepare_metadata_path, validate_tracking_state};
use crate::sources::odbc::schedule::OdbcSchedule;
use chrono_tz::Tz;
use futures_util::FutureExt;
use serde_with::DurationSeconds;
use serde_with::serde_as;
use std::collections::HashSet;
use std::fs;
use std::io::BufReader;
use std::time::Duration;
use vector_config_macros::configurable_component;
use vector_lib::config::DataType;
use vector_lib::schema;
use vector_lib::sensitive_string::SensitiveString;
use vrl::prelude::ObjectMap;
use vrl::value::{KeyString, Kind, Value, kind::Collection};

/// A positional SQL parameter for an ODBC statement placeholder (`?`).
///
/// Array order is the single source of truth for bind order and is independent of
/// configuration format key sorting.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OdbcStatementParam {
    /// Parameter name.
    ///
    /// When the same name appears in `tracking_columns`, later runs overlay the
    /// checkpointed or last-row value onto this entry while preserving bind order.
    #[configurable(metadata(docs::examples = "id"))]
    #[configurable(metadata(docs::examples = "tenant_id"))]
    pub name: String,

    /// Initial value bound for this placeholder.
    ///
    /// For non-tracking parameters this value is reused on every scheduled run.
    /// For tracking parameters it is used until a checkpoint or previous result
    /// provides an updated value.
    #[configurable(metadata(docs::examples = "0"))]
    #[configurable(metadata(docs::examples = "acme"))]
    pub value: String,
}

/// Configuration for the `odbc` source.
#[serde_as]
#[configurable_component(source(
    "odbc",
    "Periodically pulls observability data from an ODBC interface by running a scheduled query."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct OdbcConfig {
    /// The connection string to use for ODBC.
    /// If the `connection_string_filepath` is set, this value is ignored.
    #[configurable(metadata(
        docs::examples = "driver={MariaDB Unicode};server=<ip or host>;port=<port number>;database=<database name>;uid=<user>;pwd=<password>"
    ))]
    #[serde(default)]
    pub connection_string: SensitiveString,

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
    /// If this is set, the `statement` field is ignored and the file must exist and be readable.
    pub statement_filepath: Option<String>,

    /// Maximum time to allow the SQL statement to run.
    /// If the query does not finish within this window, it is canceled and retried at the next scheduled run.
    /// Set to 0 to disable the timeout and wait indefinitely.
    /// Prefer a positive timeout: Vector shutdown waits for any in-flight connect/execute, and
    /// `0` can delay exit until the ODBC driver returns.
    /// The default is 3 seconds.
    #[configurable(metadata(docs::examples = 3))]
    #[configurable(metadata(
        docs::additional_props_description = "Maximum time to wait for the SQL statement to execute"
    ))]
    #[serde(default = "default_statement_timeout_sec")]
    #[serde_as(as = "DurationSeconds<u64>")]
    pub statement_timeout: Duration,

    /// Maximum time to wait for the ODBC connection/login to complete.
    /// If the connection does not succeed within this window, the attempt fails
    /// and is retried at the next scheduled run.
    /// Set to 0 to disable the timeout and wait indefinitely.
    /// Prefer a positive timeout: Vector shutdown waits for any in-flight connect/execute, and
    /// `0` can delay exit until the ODBC driver returns.
    /// The default is 3 seconds.
    #[configurable(metadata(docs::examples = 3))]
    #[configurable(metadata(
        docs::additional_props_description = "Maximum time to wait for the ODBC connection/login to complete"
    ))]
    #[serde(default = "default_login_timeout_sec")]
    #[serde_as(as = "DurationSeconds<u64>")]
    pub login_timeout: Duration,

    /// Positional parameters for SQL statement placeholders (`?`).
    ///
    /// Array order is the bind order. Static filter values and tracking bootstrap
    /// values can be mixed; only names listed in `tracking_columns` are overlaid
    /// from checkpoints or the previous result.
    ///
    /// # Examples
    ///
    /// Incremental query with a static tenant filter:
    ///
    /// ```yaml
    /// sources:
    ///   odbc:
    ///     statement: "SELECT * FROM users WHERE tenant_id = ? AND id > ? ORDER BY id ASC"
    ///     statement_init_params:
    ///       - name: tenant_id
    ///         value: "acme"
    ///       - name: id
    ///         value: "0"
    ///     tracking_columns:
    ///       - id
    ///     last_run_metadata_path: /path/to/tracking.json
    ///     # The rest of the fields are omitted
    /// ```
    ///
    /// Static-only filter without tracking:
    ///
    /// ```yaml
    /// sources:
    ///   odbc:
    ///     statement: "SELECT * FROM users WHERE tenant_id = ?"
    ///     statement_init_params:
    ///       - name: tenant_id
    ///         value: "acme"
    ///     # The rest of the fields are omitted
    /// ```
    pub statement_init_params: Option<Vec<OdbcStatementParam>>,

    /// Cron expression used to schedule database queries. This field is required.
    #[configurable(derived)]
    pub schedule: OdbcSchedule,

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

    /// Number of rows to fetch, convert, and send per batch.
    /// This bounds ODBC driver fetch buffers and in-memory processing for each batch.
    /// Must be greater than 0.
    /// The default is 100.
    #[configurable(metadata(docs::examples = 100))]
    #[serde(default = "default_odbc_batch_size")]
    pub odbc_batch_size: usize,

    /// Maximum bytes per cell when allocating ODBC text and binary fetch buffers.
    /// Caps driver-reported sizes. Set to `0` to omit the upper bound and use
    /// driver-reported sizes instead.
    /// The default is 4096.
    #[configurable(metadata(docs::examples = 4096))]
    #[serde(default = "default_odbc_max_str_limit")]
    pub odbc_max_str_limit: usize,

    /// Timezone applied to database date/time columns that lack timezone information.
    /// Ambiguous DST times use the latest matching instant; nonexistent times are kept as text.
    /// Offset-bearing SQL or RFC3339 timestamp text is preserved as bytes and is not rewritten
    /// with this timezone, so tracking parameters can round-trip the exact ODBC text.
    /// The default is UTC.
    #[configurable(metadata(docs::examples = "UTC"))]
    #[configurable(metadata(
        docs::additional_props_description = "Timezone to use for the database date/time type without a timezone"
    ))]
    #[serde(default = "default_odbc_default_timezone")]
    pub odbc_default_timezone: Tz,

    /// Specifies the columns to track from the last row of the statement result set.
    /// Their values overlay matching entries in `statement_init_params` on later runs while
    /// preserving the declared bind order.
    ///
    /// When set, result batches are buffered until the query finishes; the final-row
    /// checkpoint is validated (and persisted when `last_run_metadata_path` is set) before
    /// any events are emitted. That avoids replaying the same rows when the last row is
    /// missing a tracking column or has an unbindable value such as null.
    /// Prefer incremental/`WHERE` bounded queries so buffering stays memory-safe.
    ///
    /// Requires `statement_init_params` entries whose names cover every tracking column.
    /// Optional `last_run_metadata_path` overlays checkpointed values onto those entries.
    /// Prefer non-binary tracking columns: checkpoints bind text parameters, so raw
    /// `BINARY`/`VARBINARY`/`BYTEA` values that are not valid UTF-8 fail validation.
    ///
    /// # Examples
    ///
    /// ```yaml
    /// sources:
    ///   odbc:
    ///     statement: "SELECT * FROM users WHERE id > ? ORDER BY id ASC"
    ///     statement_init_params:
    ///       - name: id
    ///         value: "0"
    ///     tracking_columns:
    ///       - id
    ///     # The rest of the fields are omitted
    /// ```
    #[configurable(metadata(docs::examples = "id"))]
    pub tracking_columns: Option<Vec<String>>,

    /// The path to the file where tracked column values will be saved.
    /// The tracked values are saved in JSON format and overlaid onto `statement_init_params`
    /// for the next scheduled run.
    /// If the file does not exist or the path is not specified, the initial values from
    /// `statement_init_params` are used.
    ///
    /// When tracking is enabled, the full query result is buffered, the final-row checkpoint
    /// is validated and written here, and only then are events emitted. A missing or
    /// unbindable tracking value fails the poll before any emit (avoiding infinite replay).
    /// A send failure after a successful checkpoint write may skip those rows on the next
    /// run (at-most-once). The in-memory overlay is also advanced in that case so tracking
    /// without `last_run_metadata_path` does not replay already-emitted rows. Prefer
    /// incremental queries so the buffered result stays bounded.
    ///
    /// Parent directories are created automatically if they do not exist.
    ///
    /// # Examples
    ///
    /// If `tracking_columns = ["id", "name"]`, it is saved as the following JSON data.
    ///
    /// ```json
    /// {"id":"42","name":"vector"}
    /// ```
    #[configurable(metadata(docs::examples = "/path/to/tracking.json"))]
    pub last_run_metadata_path: Option<String>,

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
    /// Trailing `\r`/`\n` from the file contents are stripped.
    pub fn connection_string_or_file(&self) -> Result<String, std::io::Error> {
        if let Some(path) = &self.connection_string_filepath {
            match fs::read_to_string(path) {
                Ok(content) => Ok(content.trim_end_matches(['\r', '\n']).to_string()),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    Ok(self.connection_string.inner().to_string())
                }
                Err(err) => Err(err),
            }
        } else {
            Ok(self.connection_string.inner().to_string())
        }
    }

    /// Returns the SQL statement to execute.
    /// If the `statement_filepath` is set, read the file and return its content.
    /// When a filepath is configured, read failures are returned instead of falling back to `statement`.
    pub fn statement_or_file(&self) -> Result<String, std::io::Error> {
        if let Some(path) = &self.statement_filepath {
            fs::read_to_string(path)
        } else if let Some(statement) = &self.statement {
            Ok(statement.clone())
        } else {
            Ok(String::new())
        }
    }

    fn validate_statement_init_params(&self) -> Result<(), String> {
        let Some(params) = &self.statement_init_params else {
            return Ok(());
        };

        if params.is_empty() {
            return Err("`statement_init_params` must not be empty when set".to_owned());
        }

        let mut seen = HashSet::with_capacity(params.len());
        for param in params {
            if param.name.trim().is_empty() {
                return Err("`statement_init_params[].name` must not be empty".to_owned());
            }
            if !seen.insert(param.name.as_str()) {
                return Err(format!(
                    "duplicate `statement_init_params` name `{}`; parameter names must be unique",
                    param.name
                ));
            }
        }

        Ok(())
    }

    fn validate_tracking_columns(&self) -> Result<(), String> {
        let has_metadata = self.last_run_metadata_path.is_some();
        let has_statement_init_params = self
            .statement_init_params
            .as_ref()
            .is_some_and(|params| !params.is_empty());
        let has_tracking_columns = self
            .tracking_columns
            .as_ref()
            .is_some_and(|columns| !columns.is_empty());

        // Checkpoint files store tracking overlays only; static-only `statement_init_params`
        // do not require tracking columns. Bind order always comes from `statement_init_params`.
        if has_metadata && !has_tracking_columns {
            return Err(
                "`tracking_columns` must be set when using `last_run_metadata_path`".to_owned(),
            );
        }

        if has_tracking_columns && !has_statement_init_params {
            return Err(
                "`statement_init_params` must be set when using `tracking_columns` so bind order is explicit"
                    .to_owned(),
            );
        }

        if let (Some(tracking_columns), Some(params)) = (
            self.tracking_columns.as_ref(),
            self.statement_init_params.as_ref(),
        ) {
            let param_names: HashSet<&str> =
                params.iter().map(|param| param.name.as_str()).collect();
            for column in tracking_columns {
                if !param_names.contains(column.as_str()) {
                    return Err(format!(
                        "`tracking_columns` entry `{column}` must also appear in `statement_init_params`"
                    ));
                }
            }
        }

        Ok(())
    }

    fn statement_init_params_as_object_map(&self) -> Option<ObjectMap> {
        self.statement_init_params.as_ref().map(|params| {
            params
                .iter()
                .map(|param| {
                    (
                        KeyString::from(param.name.as_str()),
                        Value::from(param.value.as_str()),
                    )
                })
                .collect()
        })
    }

    fn validate_tracking_bootstrap(&self) -> Result<(), String> {
        let Some(tracking_columns) = self
            .tracking_columns
            .as_ref()
            .filter(|columns| !columns.is_empty())
        else {
            return Ok(());
        };

        // `validate_tracking_columns` already requires matching `statement_init_params`.
        let init_params = self.statement_init_params_as_object_map().ok_or_else(|| {
            "`statement_init_params` must be set when using `tracking_columns`".to_owned()
        })?;
        let tz = self.odbc_default_timezone;

        if let Some(path) = &self.last_run_metadata_path {
            prepare_metadata_path(path).map_err(|error| error.to_string())?;

            return match fs::metadata(path) {
                Ok(_) => {
                    let file = fs::File::open(path).map_err(|source| {
                        format!("unable to read `last_run_metadata_path` `{path}`: {source}")
                    })?;
                    let map: ObjectMap =
                        serde_json::from_reader(BufReader::new(file)).map_err(|source| {
                            format!("unable to parse `last_run_metadata_path` `{path}`: {source}")
                        })?;
                    validate_tracking_state(&map, tracking_columns, tz)
                }
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                    validate_tracking_state(&init_params, tracking_columns, tz)
                }
                Err(source) => Err(format!(
                    "unable to access `last_run_metadata_path` `{path}`: {source}"
                )),
            };
        }

        validate_tracking_state(&init_params, tracking_columns, tz)
    }
}

impl_generate_config_from_default!(OdbcConfig);

const fn default_statement_timeout_sec() -> Duration {
    Duration::from_secs(3)
}

const fn default_login_timeout_sec() -> Duration {
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

fn default_schedule() -> OdbcSchedule {
    "0 * * * * *".into()
}

fn odbc_schema_definition(log_namespace: LogNamespace) -> schema::Definition {
    // Query rows are always objects whose columns become top-level fields. Column
    // types vary by SQL query, so unknown fields allow any VRL value including timestamps.
    let row_fields = Collection::empty().with_unknown(Kind::json().or_timestamp());

    match log_namespace {
        LogNamespace::Legacy => {
            let mut definition = schema::Definition::empty_legacy_namespace()
                .unknown_fields(Kind::json().or_timestamp());

            if let Some(timestamp_key) = log_schema().timestamp_key() {
                definition =
                    definition.try_with_field(timestamp_key, Kind::timestamp(), Some("timestamp"));
            }

            definition
        }
        LogNamespace::Vector => {
            schema::Definition::new_with_default_metadata(Kind::object(row_fields), [log_namespace])
        }
    }
}

impl Default for OdbcConfig {
    fn default() -> Self {
        Self {
            connection_string: SensitiveString::default(),
            connection_string_filepath: None,
            schedule: default_schedule(),
            schedule_timezone: Tz::UTC,
            statement: None,
            statement_timeout: default_statement_timeout_sec(),
            login_timeout: default_login_timeout_sec(),
            statement_init_params: None,
            odbc_batch_size: default_odbc_batch_size(),
            odbc_max_str_limit: default_odbc_max_str_limit(),
            odbc_default_timezone: Tz::UTC,
            tracking_columns: None,
            last_run_metadata_path: None,
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
        if self.connection_string_or_file()?.trim().is_empty() {
            return Err(
                "either a non-empty `connection_string` or a readable `connection_string_filepath` must be provided".into(),
            );
        }

        if self.statement_or_file()?.trim().is_empty() {
            return Err(
                "either a non-empty `statement` or a readable `statement_filepath` must be provided"
                    .into(),
            );
        }

        if self.odbc_batch_size == 0 {
            return Err("`odbc_batch_size` must be greater than 0".into());
        }

        self.validate_statement_init_params()?;
        self.validate_tracking_columns()?;
        self.validate_tracking_bootstrap()?;

        let log_namespace = cx.log_namespace(self.log_namespace);
        let guard = Context::new(self.clone(), cx, log_namespace)?;
        let context = Box::new(guard);
        Ok(context.run_schedule().boxed())
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let schema_definition =
            odbc_schema_definition(log_namespace).with_standard_vector_source_metadata();

        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            schema_definition,
        )]
    }

    // At-most-once when `tracking_columns` is set: the final-row checkpoint is validated and
    // persisted before events are emitted. A later send failure advances the in-memory overlay
    // (and keeps any on-disk checkpoint) so already-emitted rows are not replayed; unsent rows
    // from that poll may be skipped.
    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{OdbcConfig, OdbcStatementParam};

    #[test]
    fn parses_statement_init_params_array_preserving_order() {
        let config: OdbcConfig = toml::from_str(
            r#"
            connection_string = "driver={MariaDB Unicode};server=localhost;database=db;uid=u;pwd=p;"
            statement = "SELECT * FROM t WHERE tenant_id = ? AND id > ?"
            schedule = "*/5 * * * * *"
            statement_init_params = [
              { name = "tenant_id", value = "acme" },
              { name = "id", value = "0" },
            ]
            tracking_columns = ["id"]
            last_run_metadata_path = "tracking.json"
            "#,
        )
        .expect("parse config");

        assert_eq!(
            config.statement_init_params,
            Some(vec![
                OdbcStatementParam {
                    name: "tenant_id".to_owned(),
                    value: "acme".to_owned(),
                },
                OdbcStatementParam {
                    name: "id".to_owned(),
                    value: "0".to_owned(),
                },
            ])
        );
    }

    #[test]
    fn allows_static_statement_init_params_without_tracking() {
        let config = OdbcConfig {
            statement_init_params: Some(vec![OdbcStatementParam {
                name: "tenant_id".to_owned(),
                value: "acme".to_owned(),
            }]),
            tracking_columns: None,
            last_run_metadata_path: None,
            ..Default::default()
        };

        config
            .validate_statement_init_params()
            .expect("statement params");
        config
            .validate_tracking_columns()
            .expect("static-only params allowed");
    }

    #[test]
    fn rejects_tracking_columns_missing_from_statement_init_params() {
        let config = OdbcConfig {
            statement_init_params: Some(vec![OdbcStatementParam {
                name: "tenant_id".to_owned(),
                value: "acme".to_owned(),
            }]),
            tracking_columns: Some(vec!["id".to_owned()]),
            ..Default::default()
        };

        let error = config
            .validate_tracking_columns()
            .expect_err("tracking name must exist in statement_init_params");
        assert!(error.contains("`id`"));
    }

    #[test]
    fn rejects_duplicate_statement_param_names() {
        let config = OdbcConfig {
            statement_init_params: Some(vec![
                OdbcStatementParam {
                    name: "id".to_owned(),
                    value: "0".to_owned(),
                },
                OdbcStatementParam {
                    name: "id".to_owned(),
                    value: "1".to_owned(),
                },
            ]),
            ..Default::default()
        };

        let error = config
            .validate_statement_init_params()
            .expect_err("duplicate names");
        assert!(error.contains("duplicate"));
    }
}
