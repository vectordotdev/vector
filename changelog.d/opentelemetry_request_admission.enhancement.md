Add two optional configuration fields to the OpenTelemetry source: `max_concurrent_requests`
limits concurrent requests across HTTP and gRPC, and `request_timeout_secs` limits request
processing time. Both are disabled by default and can be enabled to help prevent out-of-memory
errors in some deployments.

authors: ArunPiduguDD
