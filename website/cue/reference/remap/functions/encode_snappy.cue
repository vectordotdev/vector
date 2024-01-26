package metadata

remap: functions: encode_snappy: {
	category: "Codec"
	description: """
		Encodes the `value` to Snappy.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to encode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` cannot be encoded into a Snappy string.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Encode to Snappy"
			source: #"""
				encoded_text = encode_snappy!("The quick brown fox jumps over 13 lazy dogs.")
				encode_base64(encoded_text)
				"""#
			return: "LKxUaGUgcXVpY2sgYnJvd24gZm94IGp1bXBzIG92ZXIgMTMgbGF6eSBkb2dzLg=="
		},
	]
}
