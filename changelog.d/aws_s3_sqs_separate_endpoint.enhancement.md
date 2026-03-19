Added optional `region` configuration to the `sqs` block of the `aws_s3` source, allowing a separate AWS region and/or endpoint override for SQS independent of the S3 region/endpoint configuration. This is useful when S3 and SQS are reachable at different endpoints (e.g., LocalStack, VPC endpoints, or cross-region SQS queues).

authors: joycse06
