The `log_to_metric` transform with `all_metrics: true` now accepts integer values wherever it expects a float (for example `counter.value`, `gauge.value`, histogram `upper_limit` and `sum`, summary `quantile`, `value` and `sum`). Previously a value such as `42` was rejected with a "Failed to parse field as float" error and the event was dropped, while `42.0` worked.

authors: anpawo
