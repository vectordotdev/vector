# Added `include_extended_tags_in_limit_metric` configuration option

Added `include_extended_tags_in_limit_metric` configuration option to the `tag_cardinality_limit` transform. When enabled, the `tag_value_limit_exceeded_total` metric includes `metric_name` and `tag_key` labels to help identify which specific metrics and tag keys are hitting the configured value limit. This option defaults to `false` to avoid high cardinality issues, and should only be enabled when needed for debugging.

authors: kaarolch
