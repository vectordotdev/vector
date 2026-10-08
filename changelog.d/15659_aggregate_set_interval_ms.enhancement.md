The `aggregate` transform now has a `set_interval_ms` option. When enabled, each flushed
incremental metric gets `interval_ms` set to the configured flush interval, so sinks such as
`datadog_metrics` send aggregated counters as rates instead of counts.

authors: gremlinops
