package metadata

base: components: sinks: blackhole: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source that supports end-to-end
				acknowledgements that is connected to that sink waits for events
				to be acknowledged by **all connected sinks** before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
		}
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
				10,
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
			1000,
		]
	}
}
