package metadata

remap: functions: decode_gzip: {
	category:    "Codec"
	description: """
		Decodes the `value` (a [Gzip](\(urls.gzip)) string) into its original string.
		"""

	arguments: [
		{
			name:        "value"
			description: "The [Gzip](\(urls.gzip)) data to decode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a valid encoded Gzip string.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode Gzip data"
			source: #"""
				encoded_text = decode_base64!("H4sIAHEAymMAA6vML1XISCxLVSguTU5OLS5OK83JqVRISU3OT0lNUchNBQD7BGDaIAAAAA==")
				decode_gzip!(encoded_text)
				"""#
			return: "you have successfully decoded me"
		},
	]
}
