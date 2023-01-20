package metadata

remap: functions: encode_zlib: {
	category:    "Codec"
	description: """
		Encodes the `value` to [Zlib](\(urls.zlib)).
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to encode."
			required:    true
			type: ["string"]
		},
		{
			name:        "compression_level"
			description: "The default compression level."
			required:    false
			type: ["integer"]
			default: 6
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Encode to Zlib"
			source: #"""
				encoded_text = encode_zlib!("please encode me")
				encode_base64(encoded_text)
				"""#
			return: "H4sIAAAAAAAA/yvISU0sTlVIzUvOT0lVyE0FAI4R4vcQAAAA"
		},
	]
}
