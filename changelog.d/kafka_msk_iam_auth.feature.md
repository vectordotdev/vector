The `kafka` source and sink now support AWS MSK IAM authentication via SASL/OAUTHBEARER. Configure it under `sasl.aws_msk_iam` with a region and the standard AWS authentication options (static keys, profile, assumed role, or IMDS). The OAuth token is a SigV4-presigned `kafka-cluster:Connect` request, generated and refreshed automatically, so no static SCRAM credentials are needed. Requires TLS (MSK IAM listens on the SASL_SSL port, `9098`).

authors: gecube
