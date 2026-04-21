Propagate FIPS endpoint setting to STS AssumeRole clients. When `AWS_USE_FIPS_ENDPOINT=true` is configured, Vector now correctly uses FIPS endpoints for STS operations (e.g., `sts-fips.<region>.amazonaws.com`) in addition to the primary service client.

authors: hligit
