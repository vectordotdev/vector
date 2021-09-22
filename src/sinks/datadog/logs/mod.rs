//! The Datadog Logs [`VectorSink`]
//!
//! This module contains the [`VectorSink`] instance that is responsible for
//! taking a stream of [`Event`] instances and getting them flung out to the
//! Datadog Log API. The log API is relatively generous in terms of its
//! constraints, except that:
//!
//!   * a 'payload' is comprised of no more than 1,000 array members
//!   * a 'payload' may not be more than 5Mb in size, uncompressed and
//!   * a 'payload' may not mix API keys
//!
//! Otherwise per [the
//! docs](https://docs.datadoghq.com/api/latest/logs/#send-logs) there aren't
//! other major constraints we have to follow in this implementation. The sink
//! is careful to always send the maximum payload size excepting where we
//! violate the size constraint.

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
