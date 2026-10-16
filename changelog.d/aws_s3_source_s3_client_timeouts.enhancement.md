The `aws_s3` source now supports `connect_timeout_seconds`, `read_timeout_seconds`, and `operation_timeout_seconds` at the top level of the source configuration to configure timeouts for the S3 client, matching the existing options under `sqs`. Previously the S3 client used for fetching objects had no timeouts, so a `GetObject` request whose response never arrived after a network disruption could stall processing until the OS gave up on the TCP connection. Timeouts remain unset by default.

authors: dbachelder
