The `datadog_metrics` sink gained a new `expire_metrics_secs` setting (matching
`prometheus_remote_write`) that evicts series not updated within that window
from its internal normalization cache, preventing unbounded memory growth
from series churn.

authors: kaarolch
