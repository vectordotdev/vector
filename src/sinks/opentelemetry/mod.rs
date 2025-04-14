//! OpenTelemetry sink[`vector_lib::sink::VectorSink`].
//!

mod config;
mod encoder;
mod request_builder;
mod service;
mod sink;

#[cfg(test)]
mod tests;
