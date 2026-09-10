package metadata

generated: components: sources: file_descriptor: configuration: {
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::2c0db0ef1f05c78303bd4391"]
	}
	fd: {
		description: "The file descriptor number to read from."
		required:    true
		type: uint: examples: [
			10
		]
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type:     _schemaDefinitions["codecs::decoding::FramingConfig"]
	}
	host_key: {
		description: """
			Overrides the name of the log field used to add the current hostname to each event.

			By default, the [global `host_key` option](https://vector.dev/docs/reference/configuration//global-options#log_schema.host_key) is used.
			"""
		required: false
		type: string: {}
	}
	max_length: {
		description: """
			The maximum buffer size, in bytes, of incoming messages.

			Messages larger than this are truncated.
			"""
		required: false
		type: uint: {
			default: 102400
			unit:    "bytes"
		}
	}
}
