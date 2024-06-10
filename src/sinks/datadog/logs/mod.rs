//! The Datadog Logs [`vector_lib::sink::VectorSink`]
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_lib::event::Event`] instances and getting them flung out to the
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
//!
//! The endpoint used to send the payload is currently being migrated from
//! `/v1/input` to `/api/v2/logs`, but the content of the above documentation
//! still applies for `/api/v2/logs`.

#[cfg(all(test, feature = "datadog-logs-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

pub mod config;
pub mod service;
pub mod sink;

pub use self::config::DatadogLogsConfig;
