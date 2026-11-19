mod config;
mod encoder;
mod encoder_v3;
mod normalizer;
mod request_builder;
mod service;
mod sink;

#[cfg(all(test, feature = "datadog-metrics-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

pub use self::config::DatadogMetricsConfig;
