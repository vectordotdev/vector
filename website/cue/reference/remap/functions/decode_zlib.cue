package metadata

remap: functions: decode_zlib: {
	category:    "Codec"
	description: """
		Decodes the `value` (a [Zlib](\(urls.zlib)) string) into its original string.
		"""

	arguments: [
		{
			name:        "value"
			description: "The [Zlib](\(urls.zlib)) data to decode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a valid encoded Zlib string.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode Zlib data"
			source: #"""
				encoded_text = decode_base64!("H4sIAHEAymMAA6vML1XISCxLVSguTU5OLS5OK83JqVRISU3OT0lNUchNBQD7BGDaIAAAAA==")
				decode_zlib!(encoded_text)
				"""#
			return: "you have successfully decoded me"
		},
	]
}
