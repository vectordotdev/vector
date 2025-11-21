# Added `internal_metrics.include_key_in_limit_metric` configuration option

Added `internal_metrics` configuration section to the `tag_cardinality_limit` transform to better organize internal metrics configuration. The `internal_metrics.include_key_in_limit_metric` option controls whether to include extended labels (`metric_name`, `tag_key`) in the `tag_value_limit_exceeded_total` metric to help identify which specific metrics and tag keys are hitting the configured value limit. This option defaults to `false` to avoid high cardinality issues, and should only be enabled when needed for debugging.

authors: kaarolch
