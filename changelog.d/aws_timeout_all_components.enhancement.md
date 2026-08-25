All AWS-backed components now bound their AWS API requests with client timeouts (`connect_timeout_seconds = 5`, `read_timeout_seconds = 30` by default), exposed as configurable `connect_timeout_seconds`, `operation_timeout_seconds`, and `read_timeout_seconds` options.

The options are now available on the `aws_cloudwatch_logs`, `aws_cloudwatch_metrics`, `aws_kinesis_firehose`, `aws_kinesis_streams`, `aws_s3`, `aws_sns`, and `aws_sqs` sinks, the `aws_s3` and `aws_sqs` sources, and the `aws_secrets_manager` secrets backend (as `client_timeout`, to avoid colliding with the unrelated `timeout` option on the `exec` secrets backend).

authors: petere-datadog
