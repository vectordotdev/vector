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
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::DeserializerConfig::2c0db0ef1f05c78303bd4391"]
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::FramingConfig::2a4f9b813a8f495175f80ccf"]
	}
	key: {
		description: "The Redis key to read messages from."
		required:    true
		type: string: examples: [
			"vector"
		]
	}
	list: {
		description: "Options for the Redis `list` data type."
		required:    false
		type: object: options: method: {
			description: "Method for getting events from the `list` data type."
			required:    true
			type: string: enum: {
				lpop: "Pop messages from the head of the list."
				rpop: "Pop messages from the tail of the list."
			}
		}
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
