HTTP-based sinks that use `http_response_retry_logic` (including the `opentelemetry`/`http` sink, `keep`, `honeycomb`, GCP Stackdriver metrics/logs, and `prometheus_remote_write`) now include a truncated (up to 1KB) response body in the "Not retriable; dropping the request" error log for non-retriable responses (e.g. `400 Bad Request`). Previously only the HTTP status code's canonical reason phrase was logged, hiding the actual error returned by the destination.

authors: peter-ehikhuemen_ddog
