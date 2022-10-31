package metadata

base: components: sinks: blackhole: configuration: {
	acknowledgements: {
		description: "Configuration of acknowledgement behavior."
		required:    false
		type: object: options: enabled: {
			description: "Enables end-to-end acknowledgements."
			required:    false
			type: bool: {}
		}
	}
	print_interval_secs: {
		description: """
			The number of seconds between reporting a summary of activity.

			Set to `0` to disable reporting.
			"""
		required: false
		type: uint: default: 1
	}
	rate: {
		description: """
			The number of events, per second, that the sink is allowed to consume.

			By default, there is no limit.
			"""
		required: false
		type: uint: {}
	}
}
