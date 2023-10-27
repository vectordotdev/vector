//! The Clickhouse [`vector_lib::sink::VectorSink`]
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_lib::event::Event`] instances and forwarding them to Clickhouse.
//!
//! Events are sent to Clickhouse using the HTTP interface with a query of the following structure:
//! `INSERT INTO my_db.my_table FORMAT JSONEachRow`. The event payload is encoded as new-line
//! delimited JSON.
//!
//! This sink only supports logs for now but could support metrics and traces as well in the future.

mod config;
#[cfg(all(test, feature = "clickhouse-integration-tests"))]
mod integration_tests;
mod service;
mod sink;
pub use self::config::ClickhouseConfig;
