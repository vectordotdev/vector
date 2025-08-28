package metadata

remap: functions: encode_lz4: {
	category:    "Codec"
	description: """
		Encodes the `value` to [Lz4](\(urls.lz4)). This function compresses the input string into an lz4 block.
		If `prepend_size` is set to `true`, it prepends the original uncompressed size to the compressed data.
		This is useful for some implementations of lz4 that require the original size to be known before decoding.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to encode."
			required:    true
			type: ["string"]
		},
		{
			name:        "prepend_size"
			description: "Whether to prepend the original size to the compressed data."
			required:    false
			default:     false
			type: ["boolean"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Encode to Lz4"
			source: #"""
				encoded_text = encode_lz4!("The quick brown fox jumps over 13 lazy dogs.")
				encode_base64(encoded_text)
				"""#
			return: "LAAAAPAdVGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIDEzIGxhenkgZG9ncy4="
		},
	]
}
