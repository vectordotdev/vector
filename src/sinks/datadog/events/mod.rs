pub mod config;
pub mod service;
pub mod sink;
pub mod request_builder;

#[cfg(test)]
mod tests;

use config::DatadogEventsConfig;
use crate::config::SinkDescription;


inventory::submit! {
    SinkDescription::new::<DatadogEventsConfig>("datadog_events")
}







