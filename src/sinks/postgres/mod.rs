mod config;
// #[cfg(all(test, feature = "postgres-integration-tests"))]
#[cfg(test)]
mod integration_tests;
mod service;
mod sink;

pub use self::config::PostgresConfig;
