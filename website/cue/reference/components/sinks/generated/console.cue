package metadata

generated: components: sinks: console: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	encoding: {
		description: """
			Encoding configuration.
			Configures how events are encoded into raw bytes.
			The selected encoding also determines which input types (logs, metrics, traces) are supported.
			"""
		required: true
		type:     _schemaDefinitions["codecs::encoding::config::EncodingConfig"]
	}
	framing: {
		description: "Framing configuration."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::framing::framer::FramingConfig"]
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
