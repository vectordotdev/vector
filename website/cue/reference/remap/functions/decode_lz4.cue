package metadata

remap: functions: decode_lz4: {
	category: "Codec"
	description: """
		Decodes the `value` (an lz4 string) into its original string.
		"""

	arguments: [
		{
			name:        "value"
			description: "The lz4 block data to decode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` unable to decode value with lz4 decoder.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode Lz4 data"
			source: #"""
				encoded_text = decode_base64!("LAAAAPAdVGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIDEzIGxhenkgZG9ncy4=")
				decode_lz4!(encoded_text)
				"""#
			return: "The quick brown fox jumps over 13 lazy dogs."
		},
	]
}
