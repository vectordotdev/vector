//! The Azure Data Explorer (ADX / Kusto) [`vector_lib::sink::VectorSink`].
//!
//! Supports both **streaming ingestion** (`POST /v1/rest/ingest/{db}/{table}`) and
//! **queued ingestion** (blob upload + Azure Queue notification).
//!
//! Events can be dynamically routed to different ADX tables based on event field
//! values or rendered templates (see `AdxPartitioner` in `sink.rs`).

mod auth;
mod config;
mod encoder;
mod request_builder;
mod resources;
mod service;
mod sink;

#[cfg(test)]
mod tests;
