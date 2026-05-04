package metadata

generated: components: sources: redis: configuration: {
	data_type: {
		description: "The Redis data type (`list` or `channel`) to use."
		required:    false
		type: string: {
			default: "list"
			enum: {
				channel: """
					The `channel` data type.

					This is based on Redis' Pub/Sub capabilities.
					"""
				list: "The `list` data type."
			}
		}
	}
	key: {
		description: "The Redis key to read messages from."
		required:    true
		type: string: examples: [
			"vector",
		]
	}
	redis_key: {
		description: """
			Sets the name of the log field to use to add the key to each event.

			The value is the Redis key that the event was read from.

			By default, this is not set and the field is not automatically added.
			"""
		required: false
		type: string: examples: ["redis_key"]
	}
	url: {
		description: """
			The Redis URL to connect to.

			The URL must take the form of `protocol://server:port/db` where the `protocol` can either be `redis` or `rediss` for connections secured using TLS.
			"""
		required: true
		type: string: examples: ["redis://127.0.0.1:6379/0"]
	}
}

generated: components: sources: redis: configuration: decoding: decodingBase & {
	type: object: options: codec: {
		required: false
		type: string: default: "bytes"
	}
}
generated: components: sources: redis: configuration: framing: framingDecoderBase & {
	type: object: options: method: {
		required: false
		type: string: default: "bytes"
	}
}
generated: components: sources: redis: configuration: list: framingEncoderBase & {
	type: object: options: method: required: true
}
