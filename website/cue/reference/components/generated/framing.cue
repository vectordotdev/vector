package metadata

framingDecoderBase: {
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
					type: ascii_char: {}
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
		chunked_gelf: {
			description:   "Options for the chunked GELF decoder."
			relevant_when: "method = \"chunked_gelf\""
			required:      false
			type: object: options: {
				decompression: {
					description: "Decompression configuration for GELF messages."
					required:    false
					type: string: {
						default: "Auto"
						enum: {
							Auto: "Automatically detect the decompression method based on the magic bytes of the message."
							Gzip: "Use Gzip decompression."
							None: "Do not decompress the message."
							Zlib: "Use Zlib decompression."
						}
					}
				}
				max_length: {
					description: """
						The maximum length of a single GELF message, in bytes. Messages longer than this length will
						be dropped. If this option is not set, the decoder does not limit the length of messages and
						the per-message memory is unbounded.

						**Note**: A message can be composed of multiple chunks and this limit is applied to the whole
						message, not to individual chunks.

						This limit takes only into account the message's payload and the GELF header bytes are excluded from the calculation.
						The message's payload is the concatenation of all the chunks' payloads.
						"""
					required: false
					type: uint: {}
				}
				pending_messages_limit: {
					description: """
						The maximum number of pending incomplete messages. If this limit is reached, the decoder starts
						dropping chunks of new messages, ensuring the memory usage of the decoder's state is bounded.
						If this option is not set, the decoder does not limit the number of pending messages and the memory usage
						of its messages buffer can grow unbounded. This matches Graylog Server's behavior.
						"""
					required: false
					type: uint: {}
				}
				timeout_secs: {
					description: """
						The timeout, in seconds, for a message to be fully received. If the timeout is reached, the
						decoder drops all the received chunks of the timed out message.
						"""
					required: false
					type: float: default: 5.0
				}
			}
		}
		length_delimited: {
			description:   "Options for the length delimited decoder."
			relevant_when: "method = \"length_delimited\""
			required:      true
			type: object: options: {
				length_field_is_big_endian: {
					description: "Length field byte order (little or big endian)"
					required:    false
					type: bool: default: true
				}
				length_field_length: {
					description: "Number of bytes representing the field length"
					required:    false
					type: uint: default: 4
				}
				length_field_offset: {
					description: "Number of bytes in the header before the length field"
					required:    false
					type: uint: default: 0
				}
				max_frame_length: {
					description: "Maximum frame length"
					required:    false
					type: uint: default: 8388608
				}
			}
		}
		max_frame_length: {
			description:   "Maximum frame length"
			relevant_when: "method = \"varint_length_delimited\""
			required:      false
			type: uint: default: 8388608
		}
		method: {
			description: "The framing method."
			type: string: enum: {
				bytes:               "Byte frames are passed through as-is according to the underlying I/O boundaries (for example, split between messages or stream segments)."
				character_delimited: "Byte frames which are delimited by a chosen character."
				chunked_gelf: """
					Byte frames which are chunked GELF messages.

					[chunked_gelf]: https://go2docs.graylog.org/current/getting_in_log_data/gelf.html
					"""
				length_delimited:  "Byte frames which are prefixed by an unsigned big-endian 32-bit integer indicating the length."
				newline_delimited: "Byte frames which are delimited by a newline character."
				octet_counting: """
					Byte frames according to the [octet counting][octet_counting] format.

					[octet_counting]: https://tools.ietf.org/html/rfc6587#section-3.4.1
					"""
				varint_length_delimited: """
					Byte frames which are prefixed by a varint indicating the length.
					This is compatible with protobuf's length-delimited encoding.
					"""
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
framingEncoderBase: {
	description: "Framing configuration."
	required:    false
	type: object: options: {
		character_delimited: {
			description:   "Options for the character delimited encoder."
			relevant_when: "method = \"character_delimited\""
			required:      true
			type: object: options: delimiter: {
				description: "The ASCII (7-bit) character that delimits byte sequences."
				required:    true
				type: ascii_char: {}
			}
		}
		length_delimited: {
			description:   "Options for the length delimited decoder."
			relevant_when: "method = \"length_delimited\""
			required:      true
			type: object: options: {
				length_field_is_big_endian: {
					description: "Length field byte order (little or big endian)"
					required:    false
					type: bool: default: true
				}
				length_field_length: {
					description: "Number of bytes representing the field length"
					required:    false
					type: uint: default: 4
				}
				length_field_offset: {
					description: "Number of bytes in the header before the length field"
					required:    false
					type: uint: default: 0
				}
				max_frame_length: {
					description: "Maximum frame length"
					required:    false
					type: uint: default: 8388608
				}
			}
		}
		max_frame_length: {
			description:   "Maximum frame length"
			relevant_when: "method = \"varint_length_delimited\""
			required:      false
			type: uint: default: 8388608
		}
		method: {
			description: "The framing method."
			type: string: enum: {
				bytes:               "Event data is not delimited at all."
				character_delimited: "Event data is delimited by a single ASCII (7-bit) character."
				length_delimited: """
					Event data is prefixed with its length in bytes.

					The prefix is a 32-bit unsigned integer, little endian.
					"""
				newline_delimited: "Event data is delimited by a newline (LF) character."
				varint_length_delimited: """
					Event data is prefixed with its length in bytes as a varint.

					This is compatible with protobuf's length-delimited encoding.
					"""
			}
		}
	}
}
