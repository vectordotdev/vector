//! The GCP Stackdriver Logs [`vector_lib::sink::VectorSink`].
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_lib::event::Event`]s and forwarding them to the GCP
//! Stackdriver service.

mod config;
mod encoder;
mod request_builder;
mod service;
mod sink;

#[cfg(test)]
mod tests;
