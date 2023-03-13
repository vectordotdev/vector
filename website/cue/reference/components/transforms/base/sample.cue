package metadata

base: components: transforms: sample: configuration: {
	exclude: {
		description: "A logical condition used to exclude events from sampling."
		required:    false
		type: condition: {}
	}
	key_field: {
		description: """
			The name of the log field whose value will be hashed to determine if the event should be
			passed.

			Consistently samples the same events. Actual rate of sampling may differ from the configured
			one if values in the field are not uniformly distributed. If left unspecified, or if the
			event doesn’t have `key_field`, events will be count rated.
			"""
		required: false
		type: string: examples: ["message"]
	}
	rate: {
		description: """
			The rate at which events will be forwarded, expressed as `1/N`.

			For example, `rate = 10` means 1 out of every 10 events will be forwarded and the rest will
			be dropped.
			"""
		required: true
		type: uint: {}
	}
}
