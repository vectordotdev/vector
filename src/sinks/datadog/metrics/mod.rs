mod config;
mod encoder;
mod normalizer;
mod request_builder;
mod service;
mod sink;

#[cfg(test)]
mod tests;

use crate::config::SinkDescription;

pub use self::config::DatadogMetricsConfig;

inventory::submit! {
    SinkDescription::new::<DatadogMetricsConfig>("datadog_metrics")
}
