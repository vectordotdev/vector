The `aggregate` transform now has a `set_interval_ms` option. When enabled, each flushed
incremental metric gets `interval_ms` set to the configured flush interval, so sinks such as
`datadog_metrics` send aggregated counters as rates instead of counts. These metrics also get
the start of their window as their timestamp: the bucket start with `event_time`, and the time
of the previous flush otherwise.

authors: gremlinops
