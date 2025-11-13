//! ODBC Data Source
//!
//! This data source runs a database query through the ODBC interface on the configured schedule.
//! Query results are sent to Vector as an array of key-value maps.
//! The final row of the result set is saved to disk and used as a parameter for the next scheduled SQL query.
//!
//! The ODBC data source offers functionality similar to the [Logstash JDBC plugin](https://www.elastic.co/docs/reference/logstash/plugins/plugins-inputs-jdbc).
//!
//! # Example
//!
//! Given the following MariaDB table and sample data:
//!
//! ```sql
//! create table odbc_table
//! (
//!     id int auto_increment primary key,
//!     name varchar(255) null,
//!     `datetime` datetime null
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
//! The example below shows how to connect to a MariaDB database with the ODBC driver,
//! run a query periodically, and send the results to Vector.
//! Provide a database connection string.
//!
//! ```toml
//! [sources.odbc]
//! type = "odbc"
//! connection_string = "driver={MariaDB Unicode};server=<your server>;port=<your port>;database=<your database>;uid=<your uid>;pwd=<your password>;"
//! statement = "SELECT * FROM odbc_table WHERE id > ? LIMIT 1;"
//! statement_init_params = { id = "0", name = "test" }
//! schedule = "*/5 * * * * *"
//! schedule_timezone = "UTC"
//! last_run_metadata_path = "/path/to/odbc_tracking.json"
//! tracking_columns = ["id"]
//!
//! [sinks.console]
//! type = "console"
//! inputs = ["odbc"]
//! encoding.codec = "json"
//! ```
//!
//! Every five seconds, the source produces output similar to the following.
//!
//! ```json
//! {"message":[{"datetime":"2025-04-28T01:20:04Z","id":1,"name":"test1"}],"timestamp":"2025-04-28T01:50:45.075484Z"}
//! {"message":[{"datetime":"2025-04-28T01:20:04Z","id":2,"name":"test2"}],"timestamp":"2025-04-28T01:50:50.017276Z"}
//! {"message":[{"datetime":"2025-04-28T01:20:04Z","id":3,"name":"test3"}],"timestamp":"2025-04-28T01:50:55.016432Z"}
//! {"message":[{"datetime":"2025-04-28T01:20:04Z","id":4,"name":"test4"}],"timestamp":"2025-04-28T01:51:00.016328Z"}
//! {"message":[{"datetime":"2025-04-28T01:20:04Z","id":5,"name":"test5"}],"timestamp":"2025-04-28T01:51:05.010063Z"}
//! ```

#[cfg(feature = "sources-odbc")]
mod client;
mod config;
#[cfg(all(test, feature = "odbc-integration-tests"))]
mod integration_tests;
mod schedule;
