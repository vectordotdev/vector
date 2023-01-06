package metadata

base: components: sources: file_descriptor: configuration: {
	fd: {
		description: "The file descriptor number to read from."
		required:    true
		type: uint: examples: [
			10,
		]
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
