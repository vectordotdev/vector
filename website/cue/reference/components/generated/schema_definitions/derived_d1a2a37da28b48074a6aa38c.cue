package metadata

_schemaDefinitions: "derived::d1a2a37da28b48074a6aa38c": object: options: {
	enabled: {
		description: "Whether or not to check the health of the sink when Vector starts up."
		required:    false
		type: bool: default: true
	}
	timeout: {
		description: "Timeout duration for healthcheck in seconds."
		required:    false
		type: float: {
			default: 10.0
			unit:    "seconds"
		}
	}
	uri: {
		description: """
			The full URI to make HTTP healthcheck requests to.

			This must be a valid URI, which requires at least the scheme and host. All other
			components -- port, path, etc -- are allowed as well.
			"""
		required: false
		type: string: {}
	}
}
