package metadata

generated: components: sinks: console: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Controls whether or not end-to-end acknowledgements are enabled.

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
	target: {
		description: """
			The [standard stream][standard_streams] to write to.

			[standard_streams]: https://en.wikipedia.org/wiki/Standard_streams
			"""
		required: false
		type: string: {
			default: "stdout"
			enum: {
				stderr: """
					Write output to [STDERR][stderr].

					[stderr]: https://en.wikipedia.org/wiki/Standard_streams#Standard_error_(stderr)
					"""
				stdout: """
					Write output to [STDOUT][stdout].

					[stdout]: https://en.wikipedia.org/wiki/Standard_streams#Standard_output_(stdout)
					"""
			}
		}
	}
}

generated: components: sinks: console: configuration: encoding: encodingBase & {
	type: object: options: codec: required: true
}
generated: components: sinks: console: configuration: framing: framingEncoderBase & {
	type: object: options: method: required: true
}
