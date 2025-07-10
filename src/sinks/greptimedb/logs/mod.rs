//! `GreptimeDB` log sink for vector.
//!
//! This sink writes Vector's log data into
//! [GreptimeDB](https://github.com/greptimeteam/greptimedb), a cloud-native
//! time-series database. It uses GreptimeDB's logs http API

mod config;
mod http_request_builder;
#[cfg(all(test, feature = "greptimedb-integration-tests"))]
mod integration_tests;
mod sink;
