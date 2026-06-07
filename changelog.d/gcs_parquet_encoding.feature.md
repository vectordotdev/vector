The `gcp_cloud_storage` sink now supports encoding events in the [Apache Parquet](https://parquet.apache.org/) columnar format, matching the existing `aws_s3` sink capability. Enable it by setting `batch_encoding.codec = "parquet"`. The Parquet format handles its own compression internally (configurable via `batch_encoding.compression`), so the top-level `compression` setting is bypassed, the object `Content-Type` is automatically set to `application/vnd.apache.parquet`, and the filename extension defaults to `parquet`.

authors: shmatovd
