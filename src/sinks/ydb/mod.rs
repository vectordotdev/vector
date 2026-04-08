//! `ydb` sink.
//!
//! YDB (Yandex Database) is an open-source distributed SQL database that combines high availability
//! and scalability with strong consistency and ACID transactions. It supports both OLTP and OLAP workloads
//! with built-in fault tolerance and automatic sharding.
//!
//! Events are inserted into YDB tables using dynamic schema mapping. The sink fetches the table schema
//! at startup and maps Vector event fields to YDB columns. Fields not present in the table are skipped.
//!
//! The sink automatically selects the optimal insertion strategy based on the table structure.
//! It uses `bulk_upsert` for high performance when possible, and falls back to transactional
//! `UPSERT` via YQL when the table has features that require it (such as secondary indexes).
//!
//! When the table schema changes (e.g., a new index is added), the sink detects errors and automatically
//! refreshes the schema to adapt its insertion strategy.
//!
//! This sink supports logs and traces. Metrics are not currently supported.
//!
//! ## References
//!
//! - [YDB Official Documentation](https://ydb.tech/docs)
//! - [YDB Rust SDK (ydb-rs-sdk)](https://docs.rs/ydb/latest/ydb/)
//! - [YDB GitHub Repository](https://github.com/ydb-platform/ydb)

mod config;
#[cfg(all(test, feature = "sinks-ydb-integration-tests"))]
mod integration_tests;
mod mapper;
mod request;
mod service;
mod sink;

pub use self::config::YdbConfig;
