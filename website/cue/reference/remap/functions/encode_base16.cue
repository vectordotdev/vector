package metadata

remap: functions: encode_base16: {
	category:    "Codec"
	description: """
		Encodes the `value` to [Base16](\(urls.base16)).
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
			title: "Encode to Base16"
			source: """
				encode_base16("please encode me")
				"""
			return: "706c6561736520656e636f6465206d65"
		},
	]
}
