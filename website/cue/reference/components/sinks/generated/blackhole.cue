package metadata

generated: components: sinks: blackhole: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	print_interval_secs: {
		description: """
			The interval between reporting a summary of activity.

			Set to `0` (default) to disable reporting.
			"""
		required: false
		type: uint: {
			default: 0
			examples: [
				10
			]
			unit: "seconds"
		}
	}
	rate: {
		description: """
			The number of events, per second, that the sink is allowed to consume.

			By default, there is no limit.
			"""
		required: false
		type: uint: examples: [
			1000
		]
	}
}
