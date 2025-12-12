The `aws_s3` sink now supports [Apache Parquet](https://parquet.apache.org/) encoding, enabling
Vector to write columnar Parquet files optimized for analytics workloads.

Parquet is a columnar storage format that provides efficient compression and encoding schemes,
making it ideal for long-term storage and query performance with tools like AWS Athena, Apache Spark,
and Presto. Users can now configure Parquet encoding with custom schemas defined directly in YAML
as a simple map of field names to data types.

Features include:
- Support for all common data types: strings (utf8), integers (int8-int64), unsigned integers,
  floats (float32, float64), timestamps (second/millisecond/microsecond/nanosecond precision),
  booleans, binary data, and decimals
- Configurable compression algorithms: snappy (default), gzip, zstd, lz4, brotli, or uncompressed

Each batch of events becomes one Parquet file in S3, with batch size controlled by the standard
`batch.max_events`, `batch.max_bytes`, and `batch.timeout_secs` settings.

authors: rorylshanks
