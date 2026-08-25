//! DuckDB table schema fetching and Arrow schema construction.

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use duckdb::Connection;
use snafu::Snafu;

#[derive(Debug, Snafu)]
pub(super) enum SchemaError {
    #[snafu(display("DuckDB error while fetching schema: {source}"))]
    DuckDb { source: duckdb::Error },

    #[snafu(display("Table '{database}.{table}' does not exist or has no columns"))]
    EmptySchema { database: String, table: String },

    #[snafu(display("Unsupported DuckDB type '{duckdb_type}' for column '{column}'"))]
    UnsupportedType { column: String, duckdb_type: String },
}

#[derive(Debug)]
struct ColumnInfo {
    name: String,
    data_type: String,
    nullable: bool,
}

pub(super) fn fetch_table_schema(
    conn: &Connection,
    database: &str,
    table: &str,
) -> Result<Schema, SchemaError> {
    let mut stmt = conn
        .prepare(
            "SELECT column_name, data_type, is_nullable \
             FROM information_schema.columns \
             WHERE table_schema = ? AND table_name = ? \
             ORDER BY ordinal_position",
        )
        .map_err(|source| SchemaError::DuckDb { source })?;

    let columns = stmt
        .query_map([database, table], |row| {
            let is_nullable: String = row.get(2)?;
            Ok(ColumnInfo {
                name: row.get(0)?,
                data_type: row.get(1)?,
                nullable: is_nullable.eq_ignore_ascii_case("YES"),
            })
        })
        .map_err(|source| SchemaError::DuckDb { source })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| SchemaError::DuckDb { source })?;

    if columns.is_empty() {
        return Err(SchemaError::EmptySchema {
            database: database.to_string(),
            table: table.to_string(),
        });
    }

    columns
        .into_iter()
        .map(|column| {
            duckdb_type_to_arrow(&column.data_type)
                .map(|data_type| Field::new(column.name.clone(), data_type, column.nullable))
                .ok_or_else(|| SchemaError::UnsupportedType {
                    column: column.name,
                    duckdb_type: column.data_type,
                })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Schema::new)
}

fn duckdb_type_to_arrow(duckdb_type: &str) -> Option<DataType> {
    let normalized = duckdb_type.trim().to_ascii_uppercase();
    let base = normalized
        .split_once('(')
        .map_or(normalized.as_str(), |(base, _)| base.trim());

    match base {
        "BOOLEAN" | "BOOL" => Some(DataType::Boolean),
        "TINYINT" => Some(DataType::Int8),
        "SMALLINT" | "INT2" | "SHORT" => Some(DataType::Int16),
        "INTEGER" | "INT" | "INT4" | "SIGNED" => Some(DataType::Int32),
        "BIGINT" | "INT8" | "LONG" => Some(DataType::Int64),
        "UTINYINT" => Some(DataType::UInt8),
        "USMALLINT" => Some(DataType::UInt16),
        "UINTEGER" => Some(DataType::UInt32),
        "UBIGINT" => Some(DataType::UInt64),
        "FLOAT" | "FLOAT4" | "REAL" => Some(DataType::Float32),
        "DOUBLE" | "FLOAT8" => Some(DataType::Float64),
        "VARCHAR" | "CHAR" | "BPCHAR" | "TEXT" | "STRING" => Some(DataType::Utf8),
        "BLOB" | "BYTEA" | "BINARY" | "VARBINARY" => Some(DataType::Binary),
        "DATE" => Some(DataType::Date32),
        "TIME" => Some(DataType::Time64(TimeUnit::Microsecond)),
        "TIMESTAMP" | "DATETIME" => Some(DataType::Timestamp(TimeUnit::Microsecond, None)),
        "TIMESTAMP_MS" => Some(DataType::Timestamp(TimeUnit::Millisecond, None)),
        "TIMESTAMP_NS" => Some(DataType::Timestamp(TimeUnit::Nanosecond, None)),
        "TIMESTAMP_S" => Some(DataType::Timestamp(TimeUnit::Second, None)),
        "DECIMAL" | "NUMERIC" => decimal_type_to_arrow(&normalized),
        _ => None,
    }
}

fn decimal_type_to_arrow(duckdb_type: &str) -> Option<DataType> {
    let parameters = duckdb_type
        .split_once('(')?
        .1
        .strip_suffix(')')?
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>();

    match parameters.as_slice() {
        [precision, scale] => Some(DataType::Decimal128(
            precision.parse().ok()?,
            scale.parse().ok()?,
        )),
        [precision] => Some(DataType::Decimal128(precision.parse().ok()?, 0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_scalar_types() {
        assert_eq!(duckdb_type_to_arrow("INTEGER"), Some(DataType::Int32));
        assert_eq!(duckdb_type_to_arrow("VARCHAR"), Some(DataType::Utf8));
        assert_eq!(duckdb_type_to_arrow("BLOB"), Some(DataType::Binary));
        assert_eq!(duckdb_type_to_arrow("DATE"), Some(DataType::Date32));
        assert_eq!(
            duckdb_type_to_arrow("TIME"),
            Some(DataType::Time64(TimeUnit::Microsecond))
        );
        assert_eq!(
            duckdb_type_to_arrow("TIMESTAMP"),
            Some(DataType::Timestamp(TimeUnit::Microsecond, None))
        );
        assert_eq!(
            duckdb_type_to_arrow("DECIMAL(18, 2)"),
            Some(DataType::Decimal128(18, 2))
        );
    }

    #[test]
    fn rejects_unsupported_types() {
        assert_eq!(duckdb_type_to_arrow("STRUCT(a INTEGER)"), None);
        assert_eq!(duckdb_type_to_arrow("UUID"), None);
    }
}
