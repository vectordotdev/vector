//! The Azure Monitor Logs [`vector_core::sink::VectorSink`]
//!
//! This module contains the [`vector_core::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_core::event::Event`] instances and forwarding them to the Azure
//! Monitor Logs service.

mod config;
mod service;
mod sink;
#[cfg(test)]
mod tests;

pub use config::AzureMonitorLogsConfig;
