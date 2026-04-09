Add Apache Parquet batch encoding support for the `aws_s3` sink with flexible schema definitions.

Events can now be encoded as Parquet columnar files with multiple schema input options:
- **Native Parquet schema** — inline string or `.schema` file

Includes configurable compression (Snappy, ZSTD, GZIP, LZ4, None) and strict/relaxed/auto_infer schema modes. 

Enable the `codecs-parquet` feature and configure `batch_encoding` with `codec = "parquet"` in the S3 sink configuration.

authors: szibis, peter-datadog
