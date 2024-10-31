//! `GreptimeDB` grpc sink for vector.
//!
//! This sink writes Vector's metric data into
//! [GreptimeDB](https://github.com/greptimeteam/greptimedb), a cloud-native
//! time-series database. It uses GreptimeDB's [gRPC
//! API](https://docs.greptime.com/user-guide/write-data/grpc) and GreptimeDB's
//! [rust client](https://github.com/GreptimeTeam/greptimedb-ingester-rust).
//!
//! This sink transforms metrics into GreptimeDB table using following rules:
//!
//! - Table name: `{namespace}_{metric_name}`. If the metric doesn't have a
//!   namespace, we will use metric_name for table name.
//! - Timestamp: timestamp is stored as a column called `ts`.
//! - Tags: metric tags are stored as string columns with its name as column
//!   name
//! - Counter and Gauge: the value of counter and gauge are stored in a column
//!   called `val`
//! - Set: the number of set items is stored in a column called `val`.
//! - Distribution, Histogram and Summary, Sketch: Statistical attributes like
//!   `sum`, `count`, "max", "min", quantiles and buckets are stored as columns.
//!

mod batch;
mod config;
#[cfg(all(test, feature = "greptimedb-integration-tests"))]
mod integration_tests;
mod request;
mod request_builder;
mod service;
mod sink;
