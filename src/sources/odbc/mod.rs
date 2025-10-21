//! ODBC Data Source
//!
//! This data source executes a database query via ODBC interface at the specified schedule.
//! The result of the database query is passed to Vector as an array of key-value pairs.
//! The last row of the result set is saved to disk and can be used as a parameter for the SQL query of the next schedule.
//!
//! The ODBC data source provides similar functionality to the [Logstash JDBC plugin](https://www.elastic.co/docs/reference/logstash/plugins/plugins-inputs-jdbc).
//!
//! # Example
//!
//! Assuming the following MySQL database table and data.
//!
//! ```sql
//! create table odbc_table
//! (
//!     id int auto_increment primary key,
//!     name varchar(255) null,
//!     datetime datetime null
//! );
//!
//! INSERT INTO odbc_table (name, datetime) VALUES
//! ('test1', now()),
//! ('test2', now()),
//! ('test3', now()),
//! ('test4', now()),
//! ('test5', now());
//! ```
//!
//! The following example shows how to connect to a MySQL database using the ODBC driver, execute a query periodically, and send the results to Vector.
//! The database connection string must be specified.
//!
//! ```toml
//! [sources.odbc]
//! type = "odbc"
//! connection_string = "driver={MariaDB Unicode};server=<your server>;port=<your port>;database=<your database>;uid=<your uid>;pwd=<your password>;"
//! statement = "SELECT * FROM odbc_table WHERE id > ? LIMIT 1;"
//! schedule = "*/5 * * * * *"
//! schedule_timezone = "UTC"
//! last_run_metadata_path = "odbc_tracking.json"
//! tracking_columns = ["id", "name", "datetime"]
//! statement_init_params = { id = "0", name = "test" }
//!
//! [sinks.console]
//! type = "console"
//! inputs = ["odbc"]
//! encoding.codec = "json"
//! ```
//!
//! The output every 5 seconds is as follows.
//!
//! ```json
//! {"message":[{"datetime":"2025-04-28T01:20:04Z","id":1,"name":"test1"}],"timestamp":"2025-04-28T01:50:45.075484Z"}
//! {"message":[{"datetime":"2025-04-28T01:20:04Z","id":2,"name":"test2"}],"timestamp":"2025-04-28T01:50:50.017276Z"}
//! {"message":[{"datetime":"2025-04-28T01:20:04Z","id":3,"name":"test3"}],"timestamp":"2025-04-28T01:50:55.016432Z"}
//! {"message":[{"datetime":"2025-04-28T01:20:04Z","id":4,"name":"test4"}],"timestamp":"2025-04-28T01:51:00.016328Z"}
//! {"message":[{"datetime":"2025-04-28T01:20:04Z","id":5,"name":"test5"}],"timestamp":"2025-04-28T01:51:05.010063Z"}
//! ```

use crate::source_sender::ClosedError;
use bytes::Bytes;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use futures::Stream;
use futures_util::stream;
use itertools::Itertools;
use odbc_api::IntoParameter;
use odbc_api::parameter::VarCharBox;
use ordered_float::NotNan;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::cell::RefCell;
use std::fmt::{Debug, Formatter};
use std::fs::File;
use std::io::BufReader;
use std::str::FromStr;
use std::{fmt, fs};
use tokio::time::sleep;
use vector_config::schema::generate_string_schema;
use vector_config::{Configurable, GenerateError, Metadata, ToValue};
use vector_config_common::schema::{SchemaGenerator, SchemaObject};
use vrl::prelude::*;

#[cfg(feature = "sources-odbc")]
mod client;

#[cfg(all(test, feature = "odbc-integration-tests"))]
mod integration_tests;

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
enum OdbcError {
    #[snafu(display("ODBC database error: {source}"))]
    Db { source: odbc_api::Error },

    #[snafu(display("File IO error: {source}"))]
    Io { source: std::io::Error },

    #[snafu(display("Batch error: {source}"))]
    Closed { source: ClosedError },

    #[snafu(display("JSON error: {source}"))]
    Json { source: serde_json::Error },

    #[snafu(display("Configuration error: {cause}"))]
    ConfigError { cause: &'static str },
}

/// Wrapper struct for Schedule.
/// Wrapper for the Schedule struct to enable Configurable implementation.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OdbcSchedule {
    inner: Schedule,
}

impl ToValue for OdbcSchedule {
    fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(&self.inner)
            .expect("Could not convert schedule(cron expression) to JSON")
    }
}

impl Configurable for OdbcSchedule {
    fn referenceable_name() -> Option<&'static str> {
        Some("cron::Schedule")
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("Cron expression in seconds.");
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl From<&str> for OdbcSchedule {
    fn from(s: &str) -> Self {
        let schedule = Schedule::from_str(s).expect("Invalid cron expression");
        Self { inner: schedule }
    }
}

impl Debug for OdbcSchedule {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner.to_string())
    }
}

impl OdbcSchedule {
    /// Creates a stream that asynchronously waits for the next scheduled cron time.
    pub(crate) fn stream(self, tz: Tz) -> impl Stream<Item = DateTime<Tz>> {
        let schedule = self.inner.clone();
        stream::unfold(schedule, move |schedule| async move {
            let now = Utc::now().with_timezone(&tz);
            let mut upcoming = schedule.upcoming(tz);
            let next = upcoming.next()?;
            let delay = (next - now).abs();

            sleep(delay.to_std().unwrap_or_default()).await;
            Some((next, schedule))
        })
    }
}

/// Loads the last result as SQL parameters.
/// Parameters are created in the order specified by `columns_order`.
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

    // Parameters are filtered by the column order.
    let params = columns_order
        .into_iter()
        .filter_map(|col| {
            let value = map.get(col)?;
            Some(value.to_string().into_parameter())
        })
        .collect_vec();

    Some(params)
}

/// Saves the last result as SQL parameters.
fn save_params(path: &str, obj: &ObjectMap) -> Result<(), OdbcError> {
    let json = serde_json::to_string(obj).context(JsonSnafu)?;
    fs::write(path, json).context(IoSnafu)
}

/// Converts ODBC data types to Vector values.
///
/// # Arguments
/// * `data_type`: The ODBC data type.
/// * `value`: The odbc value to convert.
/// * `tz`: The timezone to use for date/time conversions.
///
/// # Returns
/// A Vector value.
fn map_value(data_type: &odbc_api::DataType, value: Option<&[u8]>, tz: Tz) -> Value {
    match data_type {
        // To bytes
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

        // To integer
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

        // To float
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

        // To timestamp
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

        // To timestamp
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

        // To timestamp
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

        // To boolean
        odbc_api::DataType::Bit => {
            let Some(value) = value else {
                return Value::Null;
            };

            Value::Boolean(value[0] == 1 || value[0] == b'1')
        }
    }
}
