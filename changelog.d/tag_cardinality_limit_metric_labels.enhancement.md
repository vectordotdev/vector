# Added `internal_metrics.include_extended_tags` configuration option

Added `internal_metrics` configuration section to the `tag_cardinality_limit` transform to better organize internal metrics configuration. The `internal_metrics.include_extended_tags` option controls whether to include extended tags (`metric_name`, `tag_key`) in the `tag_value_limit_exceeded_total` metric to help identify which specific metrics and tag keys are hitting the configured value limit. This option defaults to `false` because these tags have potentially unbounded cardinality. 
authors: kaarolch
