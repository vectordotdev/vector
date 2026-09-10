package metadata

_schemaDefinitions: "vector::transforms::tag_cardinality_limit::config::InternalMetricsConfig": object: options: include_extended_tags: {
	description: """
		Whether to include extended tags (metric_name, tag_key) in the `tag_value_limit_exceeded_total` metric.

		This helps identify which metrics and tag keys are hitting cardinality limits, but can significantly
		increase metric cardinality. Defaults to `false` because these tags have potentially unbounded cardinality.
		"""
	required: false
	type: bool: default: false
}
