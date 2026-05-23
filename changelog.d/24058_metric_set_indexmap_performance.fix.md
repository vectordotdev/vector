Fixed a CPU regression introduced in 0.50.0 affecting all sinks that use metric normalization such as `prometheus_remote_write`, `aws_cloudwatch_metrics`, `statsd` and others.

The only exception is the `incremental_to_absolute` transform when `max_bytes` or `max_events` are configured, where the overhead is expected and necessary for eviction to work correctly.

authors: thomasqueirozb
