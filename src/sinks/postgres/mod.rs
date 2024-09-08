mod config;
#[cfg(all(test, feature = "postgres-integration-tests"))]
mod integration_tests;
mod service;
mod sink;

pub use self::config::PostgresConfig;
