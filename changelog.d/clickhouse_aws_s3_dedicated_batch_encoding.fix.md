The `clickhouse` and `aws_s3` sinks now use dedicated `batch_encoding` types that only expose the codecs each sink actually supports (`arrow_stream` for `clickhouse`, `parquet` for `aws_s3`). Previously the shared `BatchSerializerConfig` schema advertised codecs that were rejected at config-build time.

authors: flaviofcruz
