Bound queued and actively processing requests across the OpenTelemetry source's HTTP and gRPC
endpoints. The new `max_concurrent_requests` and `request_timeout_secs` options control overload
protection and return retryable responses when requests cannot be handled in time.

authors: ArunPiduguDD
