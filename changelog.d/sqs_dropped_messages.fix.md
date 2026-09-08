The `aws_s3` source now ensures in-flight SQS message processing completes before shutdown, preventing duplicate message delivery after visibility_timeout expires.

authors: sanjams2
