package metadata

_schemaDefinitions: "codecs::encoding::framing::framer::FramingConfig": object: options: {
	character_delimited: {
		description:   "Options for the character delimited encoder."
		relevant_when: "method = \"character_delimited\""
		required:      true
		type:          _schemaDefinitions["codecs::encoding::framing::character_delimited::CharacterDelimitedEncoderOptions"]
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
		required:    true
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
