package metadata

remap: functions: encode_json: {
	_all_types: ["boolean", "integer", "float", "string", "timestamp", "regex", "null"]

	arguments: [
		{
			name:        "value"
			description: "The value to return a json representation of."
			required:    true
			type:        _all_types
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
