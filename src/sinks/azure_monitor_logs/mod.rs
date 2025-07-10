//! The Azure Monitor Logs [`vector_lib::sink::VectorSink`]
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_lib::event::Event`] instances and forwarding them to the Azure
//! Monitor Logs service.

mod config;
mod service;
mod sink;
#[cfg(test)]
mod tests;

pub use config::AzureMonitorLogsConfig;
