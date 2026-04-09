package metadata

generated: components: sinks: journald: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
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
	journald_path: {
		description: """
			Path to the journald socket.
			If not specified, the default systemd journal socket will be used.
			"""
		required: false
		type: string: {
			default: "/run/systemd/journal/socket"
			examples: ["/run/systemd/journal/socket"]
		}
	}
}
