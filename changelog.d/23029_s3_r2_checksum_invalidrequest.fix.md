Fixed `InvalidRequest` errors ("You can only specify one non-default checksum at a time") from the `aws_s3` sink and source against S3-compatible services such as Cloudflare R2. The AWS SDK for Rust now calculates an `x-amz-checksum-*` request checksum by default, which conflicts with the `Content-MD5` header Vector already sends and which some S3-compatible providers reject outright. Request checksum calculation and response checksum validation are now restricted to only when required, matching AWS's own guidance for third-party S3-compatible endpoints.

authors: Socialpranker
