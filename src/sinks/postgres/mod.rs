mod config;
#[cfg(all(test, feature = "postgres_sink-integration-tests"))]
mod integration_tests;
mod service;
mod sink;

pub use self::config::PostgresConfig;
