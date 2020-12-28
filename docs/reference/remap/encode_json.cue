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
	return: ["string"]
	category: "Encode"

	description: "Returns the JSON representation of the argument."
	examples: [
		{
			title: "Success"
			input: {
				object: {"age": 42}
			}
			source: #"""
				.message = encode_json(.object)
				"""#
			output: {
				message: #"{"age": 42}"#
			}
		},
	]
}
