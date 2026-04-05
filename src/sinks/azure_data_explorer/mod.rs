//! The Azure Data Explorer (ADX / Kusto) [`vector_lib::sink::VectorSink`].
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_lib::event::Event`]s and forwarding them to Azure Data Explorer
//! via **streaming ingestion** (Kusto REST `POST /v1/rest/ingest/{database}/{table}`).

mod auth;
mod config;
mod encoder;
mod request_builder;
mod service;
mod sink;

#[cfg(test)]
mod tests;
