The `prometheus_exporter` sink's `flush_period_secs` option now accepts `0` to disable metric
expiration entirely. Previously, metrics with sparse or bursty updates (for example, high
cardinality counters produced by `log_to_metric`) could be expired and re-added as a "new"
series, causing gaps and apparent counter resets in downstream Prometheus queries even with a
large `flush_period_secs` configured. Setting `flush_period_secs: 0` keeps all previously seen
metrics for the lifetime of the sink; be aware this can result in unbounded memory growth if
metric series cardinality is unbounded.

authors: valerypetrov
