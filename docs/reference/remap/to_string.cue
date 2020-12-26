package metadata

remap: functions: to_string: {
	_all_types: ["boolean", "integer", "float", "string", "timestamp", "regex", "null"]

	arguments: [
		{
			name:        "value"
			description: "The value to return a string representation of."
			required:    true
			type:        _all_types
		},
		{
			name:        "default"
			description: "If the value parameter errors, return this parameter instead."
			required:    false
			type:        _all_types
		},
	]
	return: ["string"]
	category: "Coerce"
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
