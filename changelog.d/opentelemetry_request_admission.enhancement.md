Bound the number of concurrently processing requests across the OpenTelemetry source's HTTP and
gRPC endpoints. The new `max_concurrent_requests` and `request_timeout_secs` options reject excess
requests immediately, return retryable responses when admitted requests cannot be handled in time,
and expose active-request, concurrency-limit, timeout, and load-shedding metrics. The
`component_load_shed_requests_total` counter tracks requests rejected at the concurrency limit,
with a `protocol` label identifying HTTP or gRPC.

authors: ArunPiduguDD
