package metadata

base: components: transforms: throttle: configuration: {
	exclude: {
		description: "A logical condition used to exclude events from sampling."
		required:    false
		type: condition: {}
	}
	internal_metrics: {
		description: "Configuration of internal metrics for the Throttle transform."
		required:    false
		type: object: options: emit_events_discarded_per_key: {
			description: """
				Whether or not to emit the `events_discarded_total` internal metric with the `key` tag.

				If true, the counter will be incremented for each discarded event, including the key value
				associated with the discarded event. If false, the counter will not be emitted. Instead, the
				number of discarded events can be seen through the `component_discarded_events_total` internal
				metric.

				Note that this defaults to false because the `key` tag has potentially unbounded cardinality.
				Only set this to true if you know that the number of unique keys is bounded.
				"""
			required: false
			type: bool: default: false
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
