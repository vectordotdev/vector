mod config;
mod encoder;
mod normalizer;
mod request_builder;
mod service;
mod sink;

#[cfg(all(test, feature = "datadog-metrics-integration-tests"))]
mod integration_tests;

pub use self::config::DatadogMetricsConfig;
