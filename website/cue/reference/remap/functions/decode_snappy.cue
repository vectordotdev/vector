package metadata

remap: functions: decode_snappy: {
	category: "Codec"
	description: """
		Decodes the `value` (a Snappy string) into its original string.
		"""

	arguments: [
		{
			name:        "value"
			description: "The Snappy data to decode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a valid encoded Snappy string.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode Snappy data"
			source: #"""
				encoded_text = decode_base64!("LKxUaGUgcXVpY2sgYnJvd24gZm94IGp1bXBzIG92ZXIgMTMgbGF6eSBkb2dzLg==")
				decode_snappy!(encoded_text)
				"""#
			return: "The quick brown fox jumps over 13 lazy dogs."
		},
	]
}
