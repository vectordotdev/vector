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
	internal_failure_reason: null
	return: ["string"]
	category: "Encode"
	description: """
		Encodes the provided `value` into JSON.
		"""
	examples: [
		{
			title: "Encode data into JSON"
			input: log: message: age: 42
			source: #"""
				.message = encode_json(.message)
				"""#
			output: log: mesage: #"{"age": 42}"#
		},
	]
}
