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
					Whether or not to emit the `events_discarded_total` internal metric with the `key` tag.

					This legacy option applies only when `events_discarded_tags` is not configured.
					When false, discard counts remain available through `component_discarded_events_total`.
					Defaults to false because the `key` tag has potentially unbounded cardinality.
					"""
				required: false
				type: bool: default: false
			}
			events_discarded_tags: {
				description: """
					Tags to include on the `events_discarded_total` internal metric.

					Configuring this map enables the counter and replaces the legacy `key` tag selection,
					regardless of `emit_events_discarded_per_key`. An empty map emits the counter without
					attribution tags. Vector's component and collector tags are still added normally.
					When this option is absent, `emit_events_discarded_per_key` retains its existing behavior.
					To include a `key` tag, select it explicitly with a template, such as `key: "{{ workload_id }}"`.
					It is an ordinary log template, not a reference to Vector's resolved throttle key.

					Values are event templates evaluated only for discarded events. Missing fields emit a
					warning and omit that tag; empty results also omit it. These tags do not change rate-limit
					buckets or `component_discarded_events_total`. Vector's global metric tags, such as
					`component_id`, are reserved. Only use values with bounded cardinality.
					"""
				required: false
				type: object: options: "*": {
					description: "An event template for a discard metric tag."
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
