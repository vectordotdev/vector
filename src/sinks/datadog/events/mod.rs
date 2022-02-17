mod config;
pub(super) mod request_builder;
pub(crate) mod service;
pub(crate) mod sink;

#[cfg(test)]
mod tests;

use config::DatadogEventsConfig;

use crate::config::SinkDescription;

inventory::submit! {
    SinkDescription::new::<DatadogEventsConfig>("datadog_events")
}
