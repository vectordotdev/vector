pub mod config;
pub mod request_builder;
pub mod service;
pub mod sink;

#[cfg(test)]
mod tests;

use crate::config::SinkDescription;
use config::DatadogEventsConfig;

inventory::submit! {
    SinkDescription::new::<DatadogEventsConfig>("datadog_events")
}
