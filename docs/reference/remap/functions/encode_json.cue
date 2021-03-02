package metadata

remap: functions: encode_json: {
	category: "Codec"
	description: """
		Encodes the `value` to JSON.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to return a json representation of."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Encode to JSON"
			source: #"""
				.payload = encode_json({"hello": "world"})
				"""#
			return: #"{"hello": "world"}"#
		},
	]
}
