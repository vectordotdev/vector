use super::ApiKey;

#[cfg(test)]
mod tests;

mod config;
mod healthcheck;
mod service;
mod sink;

use crate::config::SinkDescription;
use crate::sinks::datadog::logs::config::DatadogLogsConfig;

inventory::submit! {
    SinkDescription::new::<DatadogLogsConfig>("datadog_logs")
}
