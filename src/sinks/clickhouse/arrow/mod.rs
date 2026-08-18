//! Schema fetching and Arrow type mapping for ClickHouse tables.

pub mod parser;
pub mod schema;

pub use schema::{ClickHouseSchemaProvider, ensure_arrow_compression_supported};
