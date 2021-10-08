mod config;
mod request_builder;
mod service;
mod sink;

use crate::config::SinkDescription;

pub use self::config::DatadogMetricsConfig;

inventory::submit! {
    SinkDescription::new::<DatadogMetricsConfig>("datadog_metrics")
}
