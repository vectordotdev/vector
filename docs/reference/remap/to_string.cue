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
	return: ["string"]
	category: "Coerce"
	description: #"""
		Returns the string representation of the first parameter. If this parameter is an error, then
		the second parameter is returned.
		"""#
	examples: [
		{
			title: "Convert number to string"
			input: log: number: 52
			source: #"""
				.number = to_string(.number)
				"""#
			output: log: number: "52"
		},
	]
}
