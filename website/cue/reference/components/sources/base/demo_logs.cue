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
		description: "Configures how events are decoded from raw bytes."
		required:    false
		type: object: options: codec: {
			description: "The codec to use for decoding events."
			required:    false
			type: string: {
				default: "bytes"
				enum: {
					bytes: "Uses the raw bytes as-is."
					gelf: """
						Decodes the raw bytes as a [GELF][gelf] message.

						[gelf]: https://docs.graylog.org/docs/gelf
						"""
					json: """
						Decodes the raw bytes as [JSON][json].

						[json]: https://www.json.org/
						"""
					native: """
						Decodes the raw bytes as Vector’s [native Protocol Buffers format][vector_native_protobuf].

						This codec is **[experimental][experimental]**.

						[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					native_json: """
						Decodes the raw bytes as Vector’s [native JSON format][vector_native_json].

						This codec is **[experimental][experimental]**.

						[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					syslog: """
						Decodes the raw bytes as a Syslog message.

						Decodes either as the [RFC 3164][rfc3164]-style format ("old" style) or the
						[RFC 5424][rfc5424]-style format ("new" style, includes structured data).

						[rfc3164]: https://www.ietf.org/rfc/rfc3164.txt
						[rfc5424]: https://www.ietf.org/rfc/rfc5424.txt
						"""
				}
			}
		}
	}
	format: {
		description: "The format of the randomly generated output."
		required:    true
		type: string: enum: {
			apache_common: """
				Randomly generated logs in [Apache common][apache_common] format.

				[apache_common]: https://httpd.apache.org/docs/current/logs.html#common
				"""
			apache_error: """
				Randomly generated logs in [Apache error][apache_error] format.

				[apache_error]: https://httpd.apache.org/docs/current/logs.html#errorlog
				"""
			bsd_syslog: """
				Randomly generated logs in Syslog format ([RFC 3164][syslog_3164]).

				[syslog_3164]: https://tools.ietf.org/html/rfc3164
				"""
			json: """
				Randomly generated HTTP server logs in [JSON][json] format.

				[json]: https://en.wikipedia.org/wiki/JSON
				"""
			shuffle: "Lines are chosen at random from the list specified using `lines`."
			syslog: """
				Randomly generated logs in Syslog format ([RFC 5424][syslog_5424]).

				[syslog_5424]: https://tools.ietf.org/html/rfc5424
				"""
		}
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
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

																By default, there is no maximum length enforced. If events are malformed, this can lead to
																additional resource usage as events continue to be buffered in memory, and can potentially
																lead to memory exhaustion in extreme cases.

																If there is a risk of processing malformed data, such as logs with user-controlled input,
																consider setting the maximum length to a reasonably large value as a safety net. This
																ensures that processing is not actually unbounded.
																"""
						required: false
						type: uint: {}
					}
				}
			}
			method: {
				description: "The framing method."
				required:    false
				type: string: {
					default: "bytes"
					enum: {
						bytes:               "Byte frames are passed through as-is according to the underlying I/O boundaries (for example, split between messages or stream segments)."
						character_delimited: "Byte frames which are delimited by a chosen character."
						length_delimited:    "Byte frames which are prefixed by an unsigned big-endian 32-bit integer indicating the length."
						newline_delimited:   "Byte frames which are delimited by a newline character."
						octet_counting: """
															Byte frames according to the [octet counting][octet_counting] format.

															[octet_counting]: https://tools.ietf.org/html/rfc6587#section-3.4.1
															"""
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

						By default, there is no maximum length enforced. If events are malformed, this can lead to
						additional resource usage as events continue to be buffered in memory, and can potentially
						lead to memory exhaustion in extreme cases.

						If there is a risk of processing malformed data, such as logs with user-controlled input,
						consider setting the maximum length to a reasonably large value as a safety net. This
						ensures that processing is not actually unbounded.
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
	interval: {
		description: """
			The amount of time, in seconds, to pause between each batch of output lines.

			The default is one batch per second. To remove the delay and output batches as quickly as possible, set
			`interval` to `0.0`.
			"""
		required: false
		type: float: {
			default: 1.0
			examples: [1.0, 0.1, 0.01]
			unit: "seconds"
		}
	}
	lines: {
		description:   "The list of lines to output."
		relevant_when: "format = \"shuffle\""
		required:      true
		type: array: items: type: string: examples: ["line1", "line2"]
	}
	sequence: {
		description:   "If `true`, each output line starts with an increasing sequence number, beginning with 0."
		relevant_when: "format = \"shuffle\""
		required:      false
		type: bool: default: false
	}
}
