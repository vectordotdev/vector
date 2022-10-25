package metadata

base: components: transforms: throttle: configuration: {
	exclude: {
		description: "A logical condition used to exclude events from sampling."
		required:    false
		type: condition: {}
	}
	key_field: {
		description: """
			The name of the log field whose value will be hashed to determine if the event should be
			rate limited.

			Each unique key will create a bucket of related events to be rate limited separately. If
			left unspecified, or if the event doesn’t have `key_field`, the event be will not be rate
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

			Each unique key will have its own `threshold`.
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
