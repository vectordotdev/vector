package metadata

base: components: transforms: throttle: configuration: {
	exclude: {
		description: "A logical condition used to exclude events from sampling."
		required:    false
		type: condition: {}
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
