package metadata

base: components: sources: exec: configuration: {
	command: {
		description: "The command to be run, plus any arguments required."
		required:    false
		type: array: {
			default: ["echo", "Hello World!"]
			items: type: string: syntax: "literal"
		}
	}
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
	include_stderr: {
		description: "Whether or not the output from stderr should be included when generating events."
		required:    false
		type: bool: default: true
	}
	maximum_buffer_size_bytes: {
		description: "The maximum buffer size allowed before a log event will be generated."
		required:    false
		type: uint: default: 1000000
	}
	mode: {
		description: "Mode of operation for running the command."
		required:    false
		type: string: {
			default: "scheduled"
			enum: {
				scheduled: "The command is run on a schedule."
				streaming: "The command is run until it exits, potentially being restarted."
			}
		}
	}
	scheduled: {
		description: "Configuration options for scheduled commands."
		required:    false
		type: object: {
			default: exec_interval_secs: 60
			options: exec_interval_secs: {
				description: """
					The interval, in seconds, between scheduled command runs.

					If the command takes longer than `exec_interval_secs` to run, it will be killed.
					"""
				required: false
				type: uint: default: 60
			}
		}
	}
	streaming: {
		description: "Configuration options for streaming commands."
		required:    false
		type: object: options: {
			respawn_interval_secs: {
				description: "The amount of time, in seconds, that Vector will wait before rerunning a streaming command that exited."
				required:    false
				type: uint: default: 5
			}
			respawn_on_exit: {
				description: "Whether or not the command should be rerun if the command exits."
				required:    false
				type: bool: default: true
			}
		}
	}
	working_directory: {
		description: "The directory in which to run the command."
		required:    false
		type: string: syntax: "literal"
	}
}
