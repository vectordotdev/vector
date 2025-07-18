package metadata

remap: functions: encode_json: {
	category: "Codec"
	description: """
		Encodes the `value` to JSON.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to convert to a JSON string."
			required:    true
			type: ["any"]
		},
		{
			name:        "pretty"
			description: "Whether to pretty print the JSON string or not."
			required:    false
			type: ["boolean"]
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
			return: #"{"hello":"world"}"#
		},
	]
}
