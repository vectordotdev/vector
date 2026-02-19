The ClickHouse sink's `arrow_stream` codec now uses `arrow-json` instead of `serde_arrow` for Arrow encoding.
`DataType::Binary` columns are no longer supported. If your ClickHouse table schema includes columns that would map to
Arrow's `Binary` type, encoding will fail. ClickHouse does not natively expose a binary column type, so this is unlikely to
affect existing configurations.

authors: benjamin-awd
