The `prometheus_remote_write` source now supports optional NaN value filtering via the `skip_nan_values` configuration option.

When enabled, metric samples with NaN values are discarded during parsing, preventing downstream processing of invalid metrics. For counters and gauges, individual samples with NaN values are filtered. For histograms and summaries, the entire metric is filtered if any component contains NaN values (sum, bucket limits, or quantile values).

This feature defaults to `false` to maintain backward compatibility.

authors: elohmeier
