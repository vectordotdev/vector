package metadata

_schemaDefinitions: "derived::64fe77681a1c274eed24cca6": object: options: {
	status_codes: {
		description:   "Retry on these specific HTTP status codes"
		relevant_when: "type = \"custom\""
		required:      true
		type: array: items: type: uint: {}
	}
	type: {
		description: "The retry strategy enum."
		required:    false
		type: string: {
			default: "default"
			enum: {
				all:     "Retry on *all* HTTP status codes except for success codes (2xx)"
				custom:  "Custom retry strategy"
				default: "Default strategy. See [`RetryStrategy::retry_action`] for more details."
				none:    "Don't retry any errors, including request timeouts."
			}
		}
	}
}
