//! Arrow encoding utilities for Vector sinks.
//!
//! This module provides generic Arrow encoding functionality that can be used
//! by any sink that supports Apache Arrow format (ClickHouse, Snowflake, Databricks, etc.).

mod encoder;

pub use encoder::{ArrowEncodingError, encode_events_to_arrow_stream};
