package metadata

remap: functions: decode_zstd: {
	category:    "Codec"
	description: """
		Decodes the `value` (a [Zstandard](\(urls.zstd)) string) into its original string.
		"""

	arguments: [
		{
			name:        "value"
			description: "The [Zstandard](\(urls.zstd)) data to decode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a valid encoded Zstd string.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode Zstd data"
			source: #"""
				encoded_text = decode_base64!("H4sIAHEAymMAA6vML1XISCxLVSguTU5OLS5OK83JqVRISU3OT0lNUchNBQD7BGDaIAAAAA==")
				decode_zstd!(encoded_text)
				"""#
			return: "you have successfully decoded me"
		},
	]
}
