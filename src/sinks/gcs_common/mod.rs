//! Shared Google Cloud Storage and Chronicle transport support.
//!
//! Requests use the native HTTP 1 client. Status-only healthcheck classification is also
//! shared with the legacy Pub/Sub sink.

pub mod config;
pub mod service;
pub mod sink;
