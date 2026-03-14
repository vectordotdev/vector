Add Apache Parquet batch encoding support for the `aws_s3` sink with flexible schema definitions.

Events can now be encoded as Parquet columnar files with multiple schema input options:
- **Inline field list** — simple flat schemas using Vector type names
- **Native Parquet schema** — inline string or `.schema` file
- **Avro schema** — inline JSON or `.avsc` file (supports nested records, arrays, maps, nullable unions)
- **Protobuf descriptor** — `.desc` file with message type (supports nested messages, maps, well-known types)

Includes configurable compression (Snappy, ZSTD, GZIP, LZ4, None) and strict/relaxed schema modes. Binary fields (inline `binary`, Avro `bytes`/`fixed`, Protobuf `bytes`, Parquet `BYTE_ARRAY` without `STRING`) are rejected at config time because the Arrow encoder cannot materialize them — use `utf8` with base64/hex encoding instead. Parquet files are optimized for analytical queries via Athena, Trino, Spark, and other columnar query engines.

Enable the `codecs-parquet` feature and configure `batch_encoding` with `codec = "parquet"` in the S3 sink configuration.

authors: szibis
