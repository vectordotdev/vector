//! The DuckDB [`vector_lib::sink::VectorSink`].
//!
//! This sink writes log events into an existing DuckDB table. The table schema is
//! read from DuckDB at startup and used to encode batches as Arrow record batches,
//! which are appended via DuckDB's appender API.

pub mod config;
#[cfg(all(test, feature = "duckdb-integration-tests"))]
mod integration_tests;
mod schema;
mod service;
mod sink;
