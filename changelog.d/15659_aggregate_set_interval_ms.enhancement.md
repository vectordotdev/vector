The `aggregate` transform now has a `set_interval_ms` option. When enabled, each flushed
incremental metric gets `interval_ms` set to the configured flush interval, so sinks such as
`datadog_metrics` send aggregated counters as rates instead of counts. Aggregated metrics with
no timestamp get the start of their flush window as their timestamp. With this option,
`interval_ms` must be a whole number of seconds.

authors: gremlinops
