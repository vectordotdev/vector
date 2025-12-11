The `aws_s3` source now emits histogram metrics to track S3 object processing times: `s3_object_processing_succeeded_duration_seconds` for successful processing and `s3_object_processing_failed_duration_seconds` for failed processing. These measure the full processing pipeline including download, decompression, and parsing. Both metrics include a `bucket` label to help identify slow buckets.

authors: sanjams2
