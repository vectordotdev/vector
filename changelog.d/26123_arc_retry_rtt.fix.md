Adaptive request concurrency now measures the round-trip time of a single request attempt and
registers a retriable failure as back pressure. Previously the concurrency limiter only observed
the final outcome of a retry sequence, so a request that was deferred and then retried into a
success recorded the retry backoff as service latency and never registered as back pressure at
all. A sink recovering from a backend outage could therefore report a round-trip time hundreds of
times the real one and drive its concurrency limit well above what the backend accepts.

authors: stigglor
