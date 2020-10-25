package metadata

remap: functions: to_string: {
	arguments: [
		{
			name:        "value"
			description: "The value to return a string representation of."
			required:    true
			type: ["any"]
		},
		{
			name:        "default"
			description: "If the value parameter errors, return this parameter instead."
			required:    false
			type: ["any"]
		},
	]
	return: ["boolean", "integer", "float", "string", "map", "array", "null"]
	category: "coerce"

	description: #"""
		Returns the string representation of the first parameter. If this parameter is an error, then
		the second parameter is returned.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				message: 52
			}
			source: #"""
				.message = to_string(.message)
				"""#
			output: {
				message: "52"
			}
		},
		{
			title: "Default"
			input: {
				message: "Some invalid JSON"
			}
			source: "to_string(parse_json(.message), 42)"
			output: {
				message: 42
			}
		},
	]
}
