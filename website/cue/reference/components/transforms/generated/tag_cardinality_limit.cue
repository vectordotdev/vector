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
		type: object: options: include_extended_tags: {
			description: """
				Whether to include extended tags (metric_name, tag_key) in the `tag_value_limit_exceeded_total` metric.

				This helps identify which metrics and tag keys are hitting cardinality limits, but can significantly
				increase metric cardinality. Defaults to `false` because these tags have potentially unbounded cardinality.
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
					type: object: options: include_extended_tags: {
						description: """
																				Whether to include extended tags (metric_name, tag_key) in the `tag_value_limit_exceeded_total` metric.

																				This helps identify which metrics and tag keys are hitting cardinality limits, but can significantly
																				increase metric cardinality. Defaults to `false` because these tags have potentially unbounded cardinality.
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
						exact: "Tracks cardinality exactly. See `Mode::Exact` for details."
						excluded: """
																			Skip cardinality tracking for this scope. All tag values pass through and nothing is
																			recorded. Other tracking fields on the entry (`value_limit`, `limit_exceeded_action`,
																			`internal_metrics`) are ignored when this is selected.

																			Only valid in `per_metric_limits` and `per_tag_limits` entries; using it as the global
																			`mode` is a configuration error.
																			"""
						probabilistic: "Tracks cardinality probabilistically. See `Mode::Probabilistic` for details."
					}
				}
				namespace: {
					description: "Namespace of the metric this configuration refers to."
					required:    false
					type: string: {}
				}
				per_tag_limits: {
					description: """
						Per-tag-key overrides scoped to this metric.

						Each entry may override `value_limit` and `mode` for a specific tag key.
						`limit_exceeded_action` and `internal_metrics` are always inherited from the enclosing
						per-metric (or global) configuration and cannot be set per-tag.
						Tags not listed here use the per-metric configuration.
						"""
					required: false
					type: object: options: "*": {
						description: "An individual tag configuration."
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
							mode: {
								description: "Controls the approach taken for tracking tag cardinality."
								required:    true
								type: string: enum: {
									exact: "Tracks cardinality exactly. See `Mode::Exact` for details."
									excluded: """
																											Skip cardinality tracking for this scope. All tag values pass through and nothing is
																											recorded. Other tracking fields on the entry (`value_limit`, `limit_exceeded_action`,
																											`internal_metrics`) are ignored when this is selected.

																											Only valid in `per_metric_limits` and `per_tag_limits` entries; using it as the global
																											`mode` is a configuration error.
																											"""
									probabilistic: "Tracks cardinality probabilistically. See `Mode::Probabilistic` for details."
								}
							}
							value_limit: {
								description: """
																								How many distinct values to accept for this tag key. If unset, inherits
																								the `value_limit` from the enclosing per-metric (or global) configuration.
																								Ignored when `mode: excluded`.
																								"""
								required: false
								type: uint: {}
							}
						}
					}
				}
				value_limit: {
					description: "How many distinct values to accept for any given key. Ignored when `mode: excluded`."
					required:    false
					type: uint: default: 500
				}
			}
		}
	}
	tracking_scope: {
		description: "Controls how tag tracking state is partitioned across metrics."
		required:    false
		type: string: {
			default: "global"
			enum: {
				global: """
					All metrics share a single tracking bucket. Tag values pool across metrics,
					and the global `value_limit` caps the combined set. Lower memory but
					cross-metric pollution.
					"""
				per_metric: """
					Every distinct metric gets its own tracking bucket, providing tag
					cardinality limiting for each metric in isolation at the cost of higher
					memory.
					"""
			}
		}
	}
	value_limit: {
		description: "How many distinct values to accept for any given key."
		required:    false
		type: uint: default: 500
	}
}
