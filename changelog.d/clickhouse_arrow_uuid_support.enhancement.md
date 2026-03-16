Added support for the ClickHouse `UUID` type in the ArrowStream format for the `clickhouse` sink. UUID columns are now automatically mapped to Arrow `Utf8` and cast by ClickHouse on insert.

authors: benjamin-awd
