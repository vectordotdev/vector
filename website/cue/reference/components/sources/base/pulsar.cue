package metadata

base: components: sources: pulsar: configuration: {
	auth: {
		description: "Authentication configuration."
		required:    false
		type: object: options: {
			name: {
				description: """
					Basic authentication name/username.

					This can be used either for basic authentication (username/password) or JWT authentication.
					When used for JWT, the value should be `token`.
					"""
				required: false
				type: string: syntax: "literal"
			}
			oauth2: {
				description: "OAuth2-specific authenticatgion configuration."
				required:    false
				type: object: options: {
					audience: {
						description: "The OAuth2 audience."
						required:    false
						type: string: syntax: "literal"
					}
					credentials_url: {
						description: """
																The credentials URL.

																A data URL is also supported.
																"""
						required: true
						type: string: syntax: "literal"
					}
					issuer_url: {
						description: "The issuer URL."
						required:    true
						type: string: syntax: "literal"
					}
					scope: {
						description: "The OAuth2 scope."
						required:    false
						type: string: syntax: "literal"
					}
				}
			}
			token: {
				description: """
					Basic authentication password/token.

					This can be used either for basic authentication (username/password) or JWT authentication.
					When used for JWT, the value should be the signed JWT, in the compact representation.
					"""
				required: false
				type: string: syntax: "literal"
			}
		}
	}
	endpoint: {
		description: "The endpoint to which the Pulsar client should connect to."
		required:    true
		type: string: syntax: "literal"
	}
	topics: {
		description: "The Pulsar topic names to read events from."
		required:    true
		type: array: items: type: string: syntax: "literal"
	}
	consumer_name: {
		description: "The Pulsar consumer name."
		required:    false
		type: string: syntax: "literal"
	}
	subscription_name: {
		description: "The Pulsar subscription name."
		required:    false
		type: string: syntax: "literal"
	}
	priority_level: {
		description: """
			Priority level for a consumer to which a broker gives more priority while dispatching messages in Shared subscription type.

            The broker follows descending priorities. For example, 0=max-priority, 1, 2,...

            In Shared subscription type, the broker first dispatches messages to the max priority level consumers if they have permits. Otherwise, the broker considers next priority level consumers.
			"""
		required: false
		type:     int
	}
	batch_size: {
		description: "Max count of messages in a batch."
		required:    false
		type:        uint
	}
	dead_letter_queue_policy: {
		description: "Dead Letter Queue policy configuration."
		required:    false
		type: object: {
			max_redeliver_count: {
				description: "Maximum number of times that a message will be redelivered before being sent to the dead letter queue."
				required:    false
				type:        uint
			}
			dead_letter_topic: {
				description: "Name of the dead topic where the failing messages will be sent."
				required:    false
				type: string: syntax: "literal"
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
}
