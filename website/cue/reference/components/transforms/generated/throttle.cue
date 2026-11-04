package metadata

generated: components: transforms: throttle: configuration: {
	exclude: {
		description: "A logical condition used to exclude events from sampling."
		required:    false
		type: condition: {}
	}
	internal_metrics: {
		description: "Configuration of internal metrics for the Throttle transform."
		required:    false
		type: object: options: {
			emit_events_discarded_per_key: {
				description: """
					Whether or not to emit the `events_discarded_total` internal metric.

					If true, the counter is incremented for each discarded event. The `key` tag is included
					by default; set `include_key_tag` to false to omit it. If false, the counter is not emitted.
					Discard counts remain available through `component_discarded_events_total`.

					This defaults to false because the `key` tag has potentially unbounded cardinality.
					Only enable the counter with tags whose cardinality is bounded.
					"""
				required: false
				type: bool: default: false
			}
			events_discarded_tags: {
				description: """
					Extra tags to add to the `events_discarded_total` internal metric.

					Values are event templates evaluated only for discarded events when
					`emit_events_discarded_per_key` is true. Missing fields emit a warning and omit that tag.
					Empty results also omit the tag.
					These tags do not change rate-limit buckets or `component_discarded_events_total`.
					The `key` tag and Vector's global metric tags, such as `component_id`, are reserved.
					Only use values with bounded cardinality to avoid creating unbounded metric series.
					"""
				required: false
				type: object: options: "*": {
					description: "An event template for an additional discard metric tag."
					required:    true
					type: string: syntax: "template"
				}
			}
			include_group_tag: {
				description: """
					Whether or not to include the `group` tag on the `component_discarded_events_total`
					internal metric.

					When enabled, adds a `group` tag containing the rendered `key_field` value.
					If the `key_field` value is missing or can't be rendered, the tag value is `None`.

					Note that this defaults to false because the `group` tag has potentially unbounded
					cardinality. Only set this to true if you know that the number of unique groups is bounded.
					"""
				required: false
				type: bool: default: false
			}
			include_key_tag: {
				description: """
					Whether to include the `key` tag on `events_discarded_total`.

					Defaults to true to preserve existing per-key metrics. The tag contains the rendered
					`key_field` value, or `None` when no key is available. Setting this to false aggregates
					discard counts across throttle keys that have the same remaining metric tags.
					This does not change rate-limit buckets or the `include_group_tag` setting.
					Requires `emit_events_discarded_per_key` to be enabled.
					"""
				required: false
				type: bool: default: true
			}
		}
	}
	key_field: {
		description: """
			The value to group events into separate buckets to be rate limited independently.

			If left unspecified, or if the event doesn't have `key_field`, then the event is not rate
			limited separately.
			"""
		required: false
		type: string: {
			examples: ["{{ message }}", "{{ hostname }}"]
			syntax: "template"
		}
	}
	threshold: {
		description: """
			The number of events allowed for a given bucket per configured `window_secs`.

			Each unique key has its own `threshold`.
			"""
		required: true
		type: uint: {}
	}
	window_secs: {
		description: "The time window in which the configured `threshold` is applied, in seconds."
		required:    true
		type: float: unit: "seconds"
	}
}
