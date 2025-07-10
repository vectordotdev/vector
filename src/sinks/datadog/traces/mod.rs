//! The Datadog Traces [`vector_lib::sink::VectorSink`]
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance responsible for taking
//! a stream of [`vector_lib::event::Event`], partition them following the right directions and
//! sending them to the Datadog Trace intake.
//! This module use the same protocol as the official Datadog trace-agent to
//! submit traces to the Datadog intake.

#[cfg(all(test, feature = "datadog-traces-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

pub(crate) mod apm_stats;
mod config;
mod request_builder;
mod service;
mod sink;

#[allow(warnings, clippy::pedantic, clippy::nursery)]
pub(crate) mod ddsketch_full {
    include!(concat!(env!("OUT_DIR"), "/ddsketch_full.rs"));
}

#[allow(warnings)]
pub(crate) mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/dd_trace.rs"));
}

pub use self::config::DatadogTracesConfig;
