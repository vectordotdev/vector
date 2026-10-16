package metadata

_schemaDefinitions: "codecs::encoding::transformer::Transformer": object: options: {
	except_fields: {
		description: "List of fields that are excluded from the encoded event."
		required:    false
		type: array: items: type: string: {}
	}
	only_fields: {
		description: "List of fields that are included in the encoded event."
		required:    false
		type: array: items: type: string: {}
	}
	timestamp_format: {
		description: "Format used for timestamp fields."
		required:    false
		type: string: enum: {
			rfc3339:    "Represent the timestamp as a RFC 3339 timestamp."
			unix:       "Represent the timestamp as a Unix timestamp."
			unix_float: "Represent the timestamp as a Unix timestamp in floating point."
			unix_ms:    "Represent the timestamp as a Unix timestamp in milliseconds."
			unix_ns:    "Represent the timestamp as a Unix timestamp in nanoseconds."
			unix_us:    "Represent the timestamp as a Unix timestamp in microseconds."
		}
	}
}
