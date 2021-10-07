mod config;
mod request_builder;
mod service;
mod sink;

use crate::config::SinkDescription;

use self::config::DatadogMetricsConfig;

inventory::submit! {
    SinkDescription::new::<DatadogMetricsConfig>("datadog_metrics")
}

impl_generate_config_from_default!(DatadogMetricsConfig);