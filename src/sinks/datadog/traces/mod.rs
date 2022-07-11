//! The Datadog Traces [`VectorSink`]
//!
//! This module contains the [`VectorSink`] instance responsible for taking
//! a stream of [`Event`], partition them following the right directions and
//! sending them to the Datadog Trace intake.
//! This module use the same protocol as the official Datadog trace-agent to
//! submit traces to the Datadog intake.

#[cfg(all(test, feature = "datadog-traces-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

mod config;
mod request_builder;
mod service;
mod sink;
mod stats;

pub(crate) mod ddsketch_full {
    include!(concat!(env!("OUT_DIR"), "/ddsketch_full.rs"));
}

pub(crate) mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/dd_trace.rs"));
}

use crate::{config::SinkDescription, sinks::datadog::traces::config::DatadogTracesConfig};

inventory::submit! {
    SinkDescription::new::<DatadogTracesConfig>("datadog_traces")
}
