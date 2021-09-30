//! The Datadog Traces [`VectorSink`]
//!
//! This module contains the [`VectorSink`] instance that is responsible for
//! taking a stream of [`Event`] instances and getting them flung out to the
//! Datadog Trace API. The trace API is relatively generous in terms of its
//! constraints, except that:
//!
//!   * a 'payload' is comprised of no more than 1,000 array members
//!   * a 'payload' may not be more than 5Mb in size, uncompressed and
//!   * a 'payload' may not mix API keys
//!
//! Otherwise per [the
//! docs](https://docs.datadoghq.com/api/latest/traces/#send-traces) there aren't
//! other major constraints we have to follow in this implementation. The sink
//! is careful to always send the maximum payload size excepting where we
//! violate the size constraint.
//!
//! The endpoint used to send the payload is currently being migrated from
//! `/v1/input` to `/api/v2/traces`, but the content of the above documentation
//! still applies for `/api/v2/traces`.

#[cfg(test)]
mod tests;

mod config;
mod request_builder;
mod service;
mod sink;

use crate::{config::SinkDescription, sinks::datadog::traces::config::DatadogTracesConfig};

inventory::submit! {
    SinkDescription::new::<DatadogTracesConfig>("datadog_traces")
}
