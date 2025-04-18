package metadata

remap: functions: encode_lz4: {
	category:    "Codec"
	description: """
		Encodes the `value` to [Lz4](\(urls.lz4)).
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to encode."
			required:    true
			type: ["string"]
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
