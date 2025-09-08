The `request_retry_partial` behavior for the `aws_kinesis_streams` was changed. Now only the failed records in a batch will be retried (instead of all records in the batch).

authors: lht
