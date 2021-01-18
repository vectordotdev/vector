package metadata

remap: functions: encode_json: {
	arguments: [
		{
			name:        "value"
			description: "The value to return a json representation of."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: ["string"]
	category: "Codec"
	description: """
		Encodes the provided `value` into JSON.
		"""
	examples: [
		{
			title: "Encode to JSON"
			input: log: payload: age: 42
			source: #"""
				.payload = encode_json(.payload)
				"""#
			output: log: payload: #"{"age": 42}"#
		},
	]
}
