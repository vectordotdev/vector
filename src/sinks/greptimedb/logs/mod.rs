//! `GreptimeDB` log sink for vector.
//!
//! This sink writes Vector's metric data into
//! [GreptimeDB](https://github.com/greptimeteam/greptimedb), a cloud-native
//! time-series database. It uses GreptimeDB's logs http API
//!
//! This sink transforms metrics into GreptimeDB table using following rules:
//!
//! - Table name: `{namespace}_{metric_name}`. If the metric doesn't have a
//! namespace, we will use metric_name for table name.
//! - Timestamp: timestamp is stored as a column called `ts`.
//! - Tags: log tags are stored as string columns with its name as column
//! name
//!

mod config;
mod http_request_builder;
mod sink;
