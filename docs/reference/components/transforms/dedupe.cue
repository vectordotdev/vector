package metadata

components: transforms: dedupe: {
	title: "Dedupe events"

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		filter: {}
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		cache: {
			common:      false
			description: "Options controlling how we cache recent Events for future duplicate checking."
			required:    false
			warnings: []
			type: object: {
				options: {
					num_events: {
						common:      true
						description: "The number of recent Events to cache and compare new incoming Events against."
						required:    false
						warnings: []
						type: uint: {
							default: 5000
							unit:    null
						}
					}
				}
			}
		}
		fields: {
			description: "Options controlling what fields to match against."
			required:    true
			warnings: []
			type: object: {
				options: {
					ignore: {
						common:      false
						description: "The field names to ignore when deciding if an Event is a duplicate. Incompatible with the `fields.match` option."
						required:    false
						warnings: []
						type: array: {
							default: null
							items: type: string: examples: ["field1", "parent.child_field"]
						}
					}
					match: {
						common:      true
						description: "The field names considered when deciding if an Event is a duplicate. This can also be globally set via the [global `log_schema` options][docs.reference.global-options#log_schema]. Incompatible with the `fields.ignore` option."
						required:    false
						warnings: []
						type: array: {
							default: ["timestamp", "host", "message"]
							items: type: string: examples: ["field1", "parent.child_field", "host", "message"]
						}
					}
				}
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {
		cache_bahavior: {
			title: "Cache Behavior"
			body: """
				This transform is backed by an LRU cache of size `cache.num_events`.
				That means that this transform will cache information in memory for
				the last `cache.num_events` Events that it has processed. Entries
				will be removed from the cache in the order they were inserted. If
				an Event is received that is considered a duplicate of an Event
				already in the cache that will put that event back to the head of
				the cache and reset its place in line, making it once again last
				entry in line to be evicted.
				"""
		}

		memory_usage_details: {
			title: "Memory Usage Details"
			body: """
				Each entry in the cache corresponds to an incoming Event and
				contains a copy of the 'value' data for all fields in the Event
				being considered for matching. When using `fields.match` this will
				be the list of fields specified in that configuration option. When
				using `fields.ignore` that will include all fields present in the
				incoming event except those specified in `fields.ignore`. Each entry
				also uses a single byte per field to store the type information of
				that field. When using `fields.ignore` each cache entry additionally
				stores a copy of each field name being considered for matching. When
				using `fields.match` storing the field names is not necessary.
				"""
		}

		memory_utilization_estimation: {
			title: "Memory Utilization Estimation"
			body: """
				If you want to estimate the memory requirements of this transform
				for your dataset, you can do so with these formulas:

				When using `fields.match`:

				```text
				Sum(the average size of the *data* (but not including the field name) for each field in `fields.match`) * `cache.num_events`
				```

				When using `fields.ignore`:

				```text
				(Sum(the average size of each incoming Event) - (the average size of the field name *and* value for each field in `fields.ignore`)) * `cache.num_events`
				```
				"""
		}

		missing_fields: {
			title: "Missing Fields"
			body: """
				Fields with explicit null values will always be considered different
				than if that field was omitted entirely. For example, if you run
				this transform with `fields.match = ["a"]`, the event "{a: null,
				b:5}" will be considered different to the event "{b:5}".
				"""
		}
	}

	telemetry: metrics: {
		vector_events_discarded_total: _vector_events_discarded_total
	}
}
