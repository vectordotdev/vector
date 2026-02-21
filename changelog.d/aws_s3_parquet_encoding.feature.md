Add Apache Parquet batch encoding support for the `aws_s3` sink.

Events can now be encoded as Parquet columnar files with user-defined schemas, configurable compression (Snappy, ZSTD, GZIP, LZ4, None), and strict/relaxed schema modes. Parquet files are optimized for analytical queries via Athena, Trino, Spark, and other columnar query engines.

Enable the `codecs-parquet` feature and configure `batch_encoding` with `codec = "parquet"` in the S3 sink configuration.

authors: szibis
