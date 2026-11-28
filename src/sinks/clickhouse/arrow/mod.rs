//! Schema fetching and Arrow type mapping for ClickHouse tables.

pub(super) mod parser;
pub(super) mod schema;

pub(super) use schema::ClickHouseSchemaProvider;
