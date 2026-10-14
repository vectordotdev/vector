Added a new `max_bytes` configuration option to the `reduce` transform. This option triggers a flush when the accumulated byte size of a reduced event group would exceed the configured threshold. This complements the existing `max_events` option and provides more granular control over memory usage.

authors: https://github.com/PGBI
