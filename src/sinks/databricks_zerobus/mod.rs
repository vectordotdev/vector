//! The Zerobus sink.
//!
//! This sink streams observability data to Databricks Unity Catalog tables
//! via the Zerobus/Shinkansen ingestion service.

mod config;
mod error;
mod service;
mod sink;
mod unity_catalog_schema;

pub use config::ZerobusSinkConfig;
