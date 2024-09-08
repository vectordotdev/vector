mod config;
mod service;
mod sink;
// #[cfg(all(test, feature = "postgres-integration-tests"))]
#[cfg(test)]
mod integration_tests;

pub use self::config::PostgresConfig;
