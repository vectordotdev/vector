The `opentelemetry` sink now supports an `instrumentation_scope` partitioning strategy that significantly improves batching and performance for OTLP data. This new strategy groups events by their InstrumentationScope (name + version) instead of URI and headers, allowing multiple ResourceLogs/ResourceMetrics/ResourceSpans with the same instrumentation scope to be batched together efficiently. This addresses poor batching efficiency when all events target the same endpoint, reducing request overhead and improving throughput.

authors: Sambhram1
