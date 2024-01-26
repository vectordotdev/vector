package metadata

remap: functions: encode_zstd: {
	category:    "Codec"
	description: """
		Encodes the `value` to [Zstandard](\(urls.zstd)).
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
			default: 3
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Encode to Zstd"
			source: #"""
				encoded_text = encode_zstd("please encode me")
				encode_base64(encoded_text)
				"""#
			return: "KLUv/QBYgQAAcGxlYXNlIGVuY29kZSBtZQ=="
		},
	]
}
