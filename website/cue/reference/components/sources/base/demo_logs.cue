package metadata

base: components: sources: demo_logs: configuration: {
	count: {
		description: """
			The total number of lines to output.

			By default, the source continuously prints logs (infinitely).
			"""
		required: false
		type: uint: default: 9223372036854775807
	}
	decoding: {
		description: "Configuration for building a `Deserializer`."
		required:    false
		type: object: {
			default: codec: "bytes"
			options: codec: {
				required: true
				type: string: enum: {
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
	format: {
		required: false
		type: string: {
			default: "json"
			enum: {
				apache_common: "Randomly generated logs in [Apache common](\\(urls.apache_common)) format."
				apache_error:  "Randomly generated logs in [Apache error](\\(urls.apache_error)) format."
				bsd_syslog:    "Randomly generated logs in Syslog format ([RFC 3164](\\(urls.syslog_3164)))."
				json:          "Randomly generated HTTP server logs in [JSON](\\(urls.json)) format."
				shuffle:       "Lines are chosen at random from the list specified using `lines`."
				syslog:        "Randomly generated logs in Syslog format ([RFC 5424](\\(urls.syslog_5424)))."
			}
		}
	}
	framing: {
		description: "Configuration for building a `Framer`."
		required:    false
		type: object: {
			default: method: "bytes"
			options: {
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
	}
	interval: {
		description: """
			The amount of time, in seconds, to pause between each batch of output lines.

			The default is one batch per second. In order to remove the delay and output batches as quickly as possible, set
			`interval` to `0.0`.
			"""
		required: false
		type: float: default: 1.0
	}
	lines: {
		description:   "The list of lines to output."
		relevant_when: "format = \"shuffle\""
		required:      true
		type: array: items: type: string: syntax: "literal"
	}
	sequence: {
		description:   "If `true`, each output line starts with an increasing sequence number, beginning with 0."
		relevant_when: "format = \"shuffle\""
		required:      false
		type: bool: default: false
	}
}
