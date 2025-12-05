package metadata

generated: components: transforms: tag_cardinality_limit: configuration: {
	cache_size_per_key: {
		description: """
			The size of the cache for detecting duplicate tags, in bytes.

			The larger the cache size, the less likely it is to have a false positive, or a case where
			we allow a new value for tag even after we have reached the configured limits.
			"""
		relevant_when: "mode = \"probabilistic\""
		required:      false
		type: uint: default: 5120
	}
	internal_metrics: {
		description: "Configuration of internal metrics for the TagCardinalityLimit transform."
		required:    false
		type: object: options: include_key_in_limit_metric: {
			description: """
				Whether to include extended labels (metric_name, tag_key) in the `tag_value_limit_exceeded_total` metric.

				This can be useful for debugging, but should be used with caution as it can significantly
				increase metric cardinality if metric names or tag keys are high cardinality.

				Note that this defaults to false because the extended tags have potentially unbounded cardinality.
				Only set this to true if you know that the number of unique metric names and tag keys is bounded.
				"""
			required: false
			type: bool: default: false
		}
	}
	limit_exceeded_action: {
		description: """
			Possible actions to take when an event arrives that would exceed the cardinality limit for one
			or more of its tags.
			"""
		required: false
		type: string: {
			default: "drop_tag"
			enum: {
				drop_event: "Drop the entire event itself."
				drop_tag:   "Drop the tag(s) that would exceed the configured limit."
			}
		}
	}
	mode: {
		description: "Controls the approach taken for tracking tag cardinality."
		required:    true
		type: string: enum: {
			exact: """
				Tracks cardinality exactly.

				This mode has higher memory requirements than `probabilistic`, but never falsely outputs
				metrics with new tags after the limit has been hit.
				"""
			probabilistic: """
				Tracks cardinality probabilistically.

				This mode has lower memory requirements than `exact`, but may occasionally allow metric
				events to pass through the transform even when they contain new tags that exceed the
				configured limit. The rate at which this happens can be controlled by changing the value of
				`cache_size_per_key`.
				"""
		}
	}
	per_metric_limits: {
		description: "Tag cardinality limits configuration per metric name."
		required:    false
		type: object: options: "*": {
			description: "An individual metric configuration."
			required:    true
			type: object: options: {
				cache_size_per_key: {
					description: """
						The size of the cache for detecting duplicate tags, in bytes.

						The larger the cache size, the less likely it is to have a false positive, or a case where
						we allow a new value for tag even after we have reached the configured limits.
						"""
					relevant_when: "mode = \"probabilistic\""
					required:      false
					type: uint: default: 5120
				}
				internal_metrics: {
					description: "Configuration of internal metrics for the TagCardinalityLimit transform."
					required:    false
					type: object: options: include_key_in_limit_metric: {
						description: """
																				Whether to include extended labels (metric_name, tag_key) in the `tag_value_limit_exceeded_total` metric.

																				This can be useful for debugging, but should be used with caution as it can significantly
																				increase metric cardinality if metric names or tag keys are high cardinality.

																				Note that this defaults to false because the extended tags have potentially unbounded cardinality.
																				Only set this to true if you know that the number of unique metric names and tag keys is bounded.
																				"""
						required: false
						type: bool: default: false
					}
				}
				limit_exceeded_action: {
					description: """
						Possible actions to take when an event arrives that would exceed the cardinality limit for one
						or more of its tags.
						"""
					required: false
					type: string: {
						default: "drop_tag"
						enum: {
							drop_event: "Drop the entire event itself."
							drop_tag:   "Drop the tag(s) that would exceed the configured limit."
						}
					}
				}
				mode: {
					description: "Controls the approach taken for tracking tag cardinality."
					required:    true
					type: string: enum: {
						exact: """
																			Tracks cardinality exactly.

																			This mode has higher memory requirements than `probabilistic`, but never falsely outputs
																			metrics with new tags after the limit has been hit.
																			"""
						probabilistic: """
																			Tracks cardinality probabilistically.

																			This mode has lower memory requirements than `exact`, but may occasionally allow metric
																			events to pass through the transform even when they contain new tags that exceed the
																			configured limit. The rate at which this happens can be controlled by changing the value of
																			`cache_size_per_key`.
																			"""
					}
				}
				namespace: {
					description: "Namespace of the metric this configuration refers to."
					required:    false
					type: string: {}
				}
				value_limit: {
					description: "How many distinct values to accept for any given key."
					required:    false
					type: uint: default: 500
				}
			}
		}
	}
	value_limit: {
		description: "How many distinct values to accept for any given key."
		required:    false
		type: uint: default: 500
	}
}
