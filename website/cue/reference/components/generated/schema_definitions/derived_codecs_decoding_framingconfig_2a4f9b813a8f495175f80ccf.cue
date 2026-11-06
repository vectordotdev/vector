package metadata

_schemaDefinitions: "derived::codecs::decoding::FramingConfig::2a4f9b813a8f495175f80ccf": object: options: {
	character_delimited: {
		description:   "Options for the character delimited decoder."
		relevant_when: "method = \"character_delimited\""
		required:      true
		type:          _schemaDefinitions["codecs::decoding::framing::character_delimited::CharacterDelimitedDecoderOptions"]
	}
	chunked_gelf: {
		description:   "Options for the chunked GELF decoder."
		relevant_when: "method = \"chunked_gelf\""
		required:      false
		type:          _schemaDefinitions["codecs::decoding::framing::chunked_gelf::ChunkedGelfDecoderOptions"]
	}
	length_delimited: {
		description:   "Options for the length delimited decoder."
		relevant_when: "method = \"length_delimited\""
		required:      true
		type:          _schemaDefinitions["codecs::common::length_delimited::LengthDelimitedCoderOptions"]
	}
	max_frame_length: {
		description:   "Maximum frame length"
		relevant_when: "method = \"varint_length_delimited\""
		required:      false
		type: uint: default: 8388608
	}
	method: {
		description: "The framing method."
		required:    false
		type: string: {
			default: "bytes"
			enum: {
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
	}
	newline_delimited: {
		description:   "Options for the newline delimited decoder."
		relevant_when: "method = \"newline_delimited\""
		required:      false
		type:          _schemaDefinitions["codecs::decoding::framing::newline_delimited::NewlineDelimitedDecoderOptions"]
	}
	octet_counting: {
		description:   "Options for the octet counting decoder."
		relevant_when: "method = \"octet_counting\""
		required:      false
		type:          _schemaDefinitions["codecs::decoding::framing::octet_counting::OctetCountingDecoderOptions"]
	}
}
