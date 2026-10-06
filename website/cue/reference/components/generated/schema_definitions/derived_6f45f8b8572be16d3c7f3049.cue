package metadata

_schemaDefinitions: "derived::6f45f8b8572be16d3c7f3049": object: options: {
	max_bytes: {
		description: """
			The maximum size of a batch that is processed by a sink.

			This is based on the uncompressed size of the batched events, before they are
			serialized or compressed.
			"""
		required: false
		type: uint: unit: "bytes"
	}
	max_events: {
		description: "The maximum size of a batch before it is flushed."
		required:    false
		type: uint: {
			default: 1
			unit:    "events"
		}
	}
	timeout_secs: {
		description: "The maximum age of a batch before it is flushed."
		required:    false
		type: float: {
			default: 1.0
			unit:    "seconds"
		}
	}
}
