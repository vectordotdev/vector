package metadata

_schemaDefinitions: "codecs::common::length_delimited::LengthDelimitedCoderOptions": object: options: {
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
