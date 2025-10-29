The `aws_s3` source now uses exponential backoff when retrying failed SQS `receive_message` operations. Previously, the source used a fixed 500ms delay between retries.

The new behavior starts at 500ms and doubles with each consecutive failure, capping at 30 seconds. This prevents excessive API calls during prolonged AWS SQS outages, invalid IAM permissions, or throttling scenarios, while still being responsive when the service recovers.

authors: medzin pront
