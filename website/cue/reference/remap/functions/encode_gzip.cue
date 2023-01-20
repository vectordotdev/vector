package metadata

remap: functions: encode_gzip: {
	category:    "Codec"
	description: """
		Encodes the `value` to [Gzip](\(urls.gzip)).
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
			title: "Encode to Gzip"
			source: #"""
				encoded_text = encode_gzip("please encode me")
				encode_base64(encoded_text)
				"""#
			return: "H4sIACblyWMAAyvISU0sTlVIzUvOT0lVyE3lAgClSiA4EQAAAA=="
		},
	]
}
