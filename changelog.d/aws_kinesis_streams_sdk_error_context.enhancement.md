The `aws_kinesis_streams` source now formats Kinesis and DynamoDB client failures with the AWS SDK for Rust `DisplayErrorContext` helper. Operational logs and returned errors include the full error cause chain (for example service exception messages) instead of only terse labels such as `service error`.

authors: benmali
