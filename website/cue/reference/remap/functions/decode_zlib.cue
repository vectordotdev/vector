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
				encoded_text = decode_base64!("eJwNy4ENwCAIBMCNXIlQ/KqplUSgCdvXAS41qPMHshCB2R1zJlWIVlR6UURX2+wx2YcuK3kAb9C1wd6dn7Fa+QH9gRxr")
				decode_zlib!(encoded_text)
				"""#
			return: "you_have_successfully_decoded_me.congratulations.you_are_breathtaking."
		},
	]
}
