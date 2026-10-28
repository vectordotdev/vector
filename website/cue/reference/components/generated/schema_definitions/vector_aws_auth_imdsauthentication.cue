package metadata

_schemaDefinitions: "vector::aws::auth::ImdsAuthentication": object: options: {
	connect_timeout_seconds: {
		description: "Connect timeout for IMDS."
		required:    false
		type: uint: {
			default: 1
			unit:    "seconds"
		}
	}
	max_attempts: {
		description: "Number of IMDS retries for fetching tokens and metadata."
		required:    false
		type: uint: default: 4
	}
	read_timeout_seconds: {
		description: "Read timeout for IMDS."
		required:    false
		type: uint: {
			default: 1
			unit:    "seconds"
		}
	}
}
