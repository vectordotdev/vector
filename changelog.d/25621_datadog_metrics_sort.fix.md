Fixed a bug in the `datadog_metrics` sink where the metric type name was compared against itself (instead of the peer metric) when sorting metrics before encoding. The sort key is `(type_name, metric_name, timestamp)`, but the type comparison was a no-op, making `metric_name` the effective primary key. The fix restores the intended ordering.

authors: gwenaskell
