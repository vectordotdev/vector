The `aws_s3` source now emits a histogram metric `s3_object_processing_duration_seconds` to track S3 object processing times. This measures the full processing pipeline including download, decompression, and parsing. The metric includes `bucket` and `status` labels to help identify slow buckets and distinguish successful vs failed processing.

authors: sanjams2
