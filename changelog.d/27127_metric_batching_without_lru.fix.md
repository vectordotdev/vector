Reduced CPU usage of metric batching in the `aws_cloudwatch_metrics`, `sematext_metrics` and `influxdb_metrics` sinks. Since 0.50.0, these sinks did LRU cache bookkeeping on every metric, but they never evicted metrics from the cache.

authors: thomasqueirozb
