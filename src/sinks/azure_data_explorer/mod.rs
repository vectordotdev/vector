//! The Azure Data Explorer (ADX / Kusto) [`vector_lib::sink::VectorSink`].
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_lib::event::Event`]s and forwarding them to Azure Data Explorer
//! via **queued ingestion** (blob upload + queue notification), matching the Fluent Bit
//! `out_azure_kusto` plugin's ingestion flow.

mod auth;
mod config;
mod encoder;
mod request_builder;
mod resources;
mod service;
mod sink;

#[cfg(test)]
mod tests;
