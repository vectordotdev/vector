package metadata

remap: functions: decode_lz4: {
	category: "Codec"
	description: """
		Decodes the `value` (an lz4 string) into its original string. `buf_size` is the size of the buffer to decode into, this must be equal to or larger than the uncompressed size.
		If `prepended_size` is set to `true`, it expects the original uncompressed size to be prepended to the compressed data.
		`prepended_size` is useful for some implementations of lz4 that require the original size to be known before decoding.
		"""

	arguments: [
		{
			name:        "value"
			description: "The lz4 block data to decode."
			required:    true
			type: ["string"]
		},
		{
			name:        "buf_size"
			description: "The size of the buffer to decode into, this must be equal to or larger than the uncompressed size."
			required:    false
			default:     1024 * 1024 // 1 MiB
			type: ["integer"]
		},
		{
			name:        "prepended_size"
			description: "Some implementations of lz4 require the original uncompressed size to be prepended to the compressed data."
			required:    false
			default:     false
			type: ["boolean"]
		},
	]
	internal_failure_reasons: [
		"`value` unable to decode value with lz4 frame decoder.",
		"`value` unable to decode value with lz4 block decoder.",
		"`value` unable to decode because the output is too large for the buffer.",
		"`value` unable to decode because the prepended size is not a valid integer.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode Lz4 data with prepended size."
			source: #"""
				encoded_text = decode_base64!("LAAAAPAdVGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIDEzIGxhenkgZG9ncy4=")
				decode_lz4!(encoded_text, use_prepended_size: true)
				"""#
			return: "The quick brown fox jumps over 13 lazy dogs."
		},
		{
			title: "Decode Lz4 data without prepended size."
			source: #"""
				encoded_text = decode_base64!("8B1UaGUgcXVpY2sgYnJvd24gZm94IGp1bXBzIG92ZXIgMTMgbGF6eSBkb2dzLg==")
				decode_lz4!(encoded_text)
				"""#
			return: "The quick brown fox jumps over 13 lazy dogs."
		},
	]
}
