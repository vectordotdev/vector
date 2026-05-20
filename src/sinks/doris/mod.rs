//! The Doris [`vector_lib::sink::VectorSink`]
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_lib::event::Event`] instances and forwarding them to Apache Doris.
//!
//! Events are sent to Doris using the HTTP interface with Stream Load protocol. The event payload
//! is encoded as new-line delimited JSON or other formats specified by the user.
//!
//! This sink only supports logs for now but could support metrics and traces as well in the future.

mod common;
mod config;
mod health;
mod request_builder;
mod retry;
mod service;
mod sink;

#[cfg(all(test, feature = "doris-integration-tests"))]
mod integration_test;

mod client;

pub use self::config::DorisConfig;
