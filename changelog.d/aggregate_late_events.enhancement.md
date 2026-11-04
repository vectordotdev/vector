The `aggregate` transform now supports an `event_time.late_events` option to control metrics that arrive after their bucket's cutoff. Setting it to `forward` sends them downstream unchanged instead of dropping them, and `forward_distribution` forwards only late `distribution`, `aggregated_histogram`, and `sketch` metrics while dropping the rest. The default, `drop`, keeps the existing behavior.

authors: dd-sebastien-lb
