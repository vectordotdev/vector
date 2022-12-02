package metadata

base: components: sources: redis: configuration: {
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
		description: "Configuration for building a `Deserializer`."
		required:    false
		type: object: options: codec: {
			required: false
			type: string: {
				default: "bytes"
				enum: {
					bytes:       "Configures the `BytesDeserializer`."
					gelf:        "Configures the `GelfDeserializer`."
					json:        "Configures the `JsonDeserializer`."
					native:      "Configures the `NativeDeserializer`."
					native_json: "Configures the `NativeJsonDeserializer`."
					syslog:      "Configures the `SyslogDeserializer`."
				}
			}
		}
	}
	framing: {
		description: "Configuration for building a `Framer`."
		required:    false
		type: object: options: {
			character_delimited: {
				description:   "Options for the character delimited decoder."
				relevant_when: "method = \"character_delimited\""
				required:      true
				type: object: options: {
					delimiter: {
						description: "The character that delimits byte sequences."
						required:    true
						type: uint: {}
					}
					max_length: {
						description: """
																The maximum length of the byte buffer.

																This length does *not* include the trailing delimiter.
																"""
						required: false
						type: uint: {}
					}
				}
			}
			method: {
				required: false
				type: string: {
					default: "bytes"
					enum: {
						bytes:               "Configures the `BytesDecoder`."
						character_delimited: "Configures the `CharacterDelimitedDecoder`."
						length_delimited:    "Configures the `LengthDelimitedDecoder`."
						newline_delimited:   "Configures the `NewlineDelimitedDecoder`."
						octet_counting:      "Configures the `OctetCountingDecoder`."
					}
				}
			}
			newline_delimited: {
				description:   "Options for the newline delimited decoder."
				relevant_when: "method = \"newline_delimited\""
				required:      false
				type: object: options: max_length: {
					description: """
						The maximum length of the byte buffer.

						This length does *not* include the trailing delimiter.
						"""
					required: false
					type: uint: {}
				}
			}
			octet_counting: {
				description:   "Options for the octet counting decoder."
				relevant_when: "method = \"octet_counting\""
				required:      false
				type: object: options: max_length: {
					description: "The maximum length of the byte buffer."
					required:    false
					type: uint: {}
				}
			}
		}
	}
	key: {
		description: "The Redis key to read messages from."
		required:    true
		type: string: syntax: "literal"
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

			The value will be the Redis key that the event was read from.

			By default, this is not set and the field will not be automatically added.
			"""
		required: false
		type: string: syntax: "literal"
	}
	url: {
		description: """
			The Redis URL to connect to.

			The URL must take the form of `protocol://server:port/db` where the `protocol` can either be `redis` or `rediss` for connections secured via TLS.
			"""
		required: true
		type: string: syntax: "literal"
	}
}
