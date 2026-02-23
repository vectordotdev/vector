mod config;
#[cfg(all(test, feature = "sinks-ydb-integration-tests"))]
mod integration_tests;
mod service;
mod sink;

pub use self::config::YdbConfig;
