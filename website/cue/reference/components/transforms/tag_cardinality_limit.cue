package metadata

components: transforms: tag_cardinality_limit: {
	title: "Tag Cardinality Limit"

	description: """
		Limits the cardinality of tags on metric events, protecting against
		accidental high cardinality usage that can commonly disrupt the stability
		of metrics storages.

		The default behavior is to drop the tag from incoming metrics when the configured
		limit would be exceeded. Note that this is usually only useful when applied to
		incremental counter metrics and can have unintended effects when applied to other
		metric types. The default action to take can be modified with the
		`limit_exceeded_action` option.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		filter: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	// TODO: It'd be nice to have a way to define the description of the enum tag field on the Rust
	// side and propagate it forward, since this is a common pattern that gets used.
	configuration: base.components.transforms.tag_cardinality_limit.configuration & {
		mode: description: "Controls the approach taken for tracking tag cardinality."
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: false
	}

	examples: [
		{
			title: "Drop high-cardinality tag"
			notes: """
				In this example we'll demonstrate how to drop a
				high-cardinality tag named `user_id`. Notice that the
				second metric's `user_id` tag has been removed. That's
				because it exceeded the `value_limit`.
				"""
			configuration: {
				value_limit:           1
				limit_exceeded_action: "drop_tag"
			}
			input: [
				{metric: {
					kind: "incremental"
					name: "logins"
					counter: {
						value: 2.0
					}
					tags: {
						user_id: "user_id_1"
					}
				}},
				{metric: {
					kind: "incremental"
					name: "logins"
					counter: {
						value: 2.0
					}
					tags: {
						user_id: "user_id_2"
					}
				}},
			]
			output: [
				{metric: {
					kind: "incremental"
					name: "logins"
					counter: {
						value: 2.0
					}
					tags: {
						user_id: "user_id_1"
					}
				}},
				{metric: {
					kind: "incremental"
					name: "logins"
					counter: {
						value: 2.0
					}
					tags: {}
				}},
			]
		},
	]

	how_it_works: {
		intended_usage: {
			title: "Intended Usage"
			body: """
				This transform is intended to be used as a protection mechanism to prevent
				upstream mistakes. Such as a developer accidentally adding a `request_id`
				tag. When this is happens, it is recommended to fix the upstream error as soon
				as possible. This is because Vector's cardinality cache is held in memory and it
				will be erased when Vector is restarted. This will cause new tag values to pass
				through until the cardinality limit is reached again. For normal usage this
				should not be a common problem since Vector processes are normally long-lived.
				"""
		}

		memory_utilization: {
			title: "Failed Parsing"
			body: """
				This transform stores in memory a copy of the key for every tag on every metric
				event seen by this transform.  In mode `exact`, a copy of every distinct
				value *for each key* is also kept in memory, until `value_limit` distinct values
				have been seen for a given key, at which point new values for that key will be
				rejected.  So to estimate the memory usage of this transform in mode `exact`
				you can use the following formula:

				```text
				(number of distinct field names in the tags for your metrics * average length of
				the field names for the tags) + (number of distinct field names in the tags of
				your metrics * `value_limit` * average length of the values of tags for your
				metrics)
				```

				In mode `probabilistic`, rather than storing all values seen for each key, each
				distinct key has a bloom filter which can probabilistically determine whether
				a given value has been seen for that key.  The formula for estimating memory
				usage in mode `probabilistic` is:

				```text
				(number of distinct field names in the tags for your metrics * average length of
				the field names for the tags) + (number of distinct field names in the tags of
				-your metrics * `cache_size_per_key`)
				```

				The `cache_size_per_key` option controls the size of the bloom filter used
				for storing the set of acceptable values for any single key. The larger the
				bloom filter the lower the false positive rate, which in our case means the less
				likely we are to allow a new tag value that would otherwise violate a
				configured limit. If you want to know the exact false positive rate for a given
				`cache_size_per_key` and `value_limit`, there are many free on-line bloom filter
				calculators that can answer this. The formula is generally presented in terms of
				'n', 'p', 'k', and 'm' where 'n' is the number of items in the filter
				(`value_limit` in our case), 'p' is the probability of false positives (what we
				want to solve for), 'k' is the number of hash functions used internally, and 'm'
				is the number of bits in the bloom filter. You should be able to provide values
				for just 'n' and 'm' and get back the value for 'p' with an optimal 'k' selected
				for you.   Remember when converting from `value_limit` to the 'm' value to plug
				into the calculator that `value_limit` is in bytes, and 'm' is often presented
				in bits (1/8 of a byte).
				"""
		}

		restarts: {
			title: "Restarts"
			body: """
				This transform's cache is held in memory, and therefore, restarting Vector
				will reset the cache. This means that new values will be passed through until
				the cardinality limit is reached again. See [intended usage](#intended-usage)
				for more info.
				"""
		}
	}

	telemetry: metrics: {
		tag_value_limit_exceeded_total: components.sources.internal_metrics.output.metrics.tag_value_limit_exceeded_total
		value_limit_reached_total:      components.sources.internal_metrics.output.metrics.value_limit_reached_total
	}
}
