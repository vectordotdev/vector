The `aws_s3` source now supports configuring separate authentication credentials for SQS via the new `sqs.auth` configuration option. This enables cross-account scenarios where S3 buckets and SQS queues are in different AWS accounts or require different permission models.

When `sqs.auth` is not specified, the source falls back to using the main `auth` configuration, maintaining full backwards compatibility with existing deployments.

The `sqs.deferred.auth` option is also available for configuring separate authentication for the deferred message queue.

authors: kfir
