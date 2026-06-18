Add support for optionally applying rate limiting to the `internal_logs` source controlled by the
`--internal-logs-source-rate-limit` CLI option and `VECTOR_INTERNAL_LOGS_SOURCE_RATE_LIMIT`
environment variable. This provides the same rate limiting functionality as was available before
version 0.51.1 but with a rate limit window separate from the console one.

authors: bruceg
