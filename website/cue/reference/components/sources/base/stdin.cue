package metadata

base: components: sources: stdin: configuration: {
	decoding: {
		description: "Configuration for building a `Deserializer`."
		required:    false
		type: object: {
			default: codec: "bytes"
			options: codec: {
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
				required: true
				type: string: enum: {
					bytes:               "Configures the `BytesDecoder`."
					character_delimited: "Configures the `CharacterDelimitedDecoder`."
					length_delimited:    "Configures the `LengthDelimitedDecoder`."
					newline_delimited:   "Configures the `NewlineDelimitedDecoder`."
					octet_counting:      "Configures the `OctetCountingDecoder`."
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
	host_key: {
		description: """
			Overrides the name of the log field used to add the current hostname to each event.

			The value will be the current hostname for wherever Vector is running.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: syntax: "literal"
	}
	max_length: {
		description: """
			The maximum buffer size, in bytes, of incoming messages.

			Messages larger than this are truncated.
			"""
		required: false
		type: uint: default: 102400
	}
}
