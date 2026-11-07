The `aggregate` transform now supports an `event_time.late_events` option to control metrics that arrive after their bucket's cutoff. Setting `strategy` to `forward` sends them downstream unchanged instead of dropping them, and the optional `metric_types` list restricts forwarding to incremental metrics of the listed types (for example `distribution` and `sketch`). The default strategy, `drop`, keeps the existing behavior.

authors: dd-sebastien-lb
