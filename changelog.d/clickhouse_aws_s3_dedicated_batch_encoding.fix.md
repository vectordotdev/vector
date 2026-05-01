The `aws_s3` and `clickhouse` sinks now correctly advertise only the `batch_encoding.codec` values they actually support: `parquet` for `aws_s3` and `arrow_stream` for `clickhouse`. Previously the documentation and configuration schema listed both codecs for both sinks, even though picking the wrong one produced a startup error.

authors: flaviofcruz
