package metadata

remap: functions: upcase: {
	arguments: [
		{
			name:        "value"
			description: "The string to convert to uppercase."
			required:    true
			type: ["string"]
		},
	]
	return: ["string"]
	category: "String"
	description: #"""
		Returns a copy of `string` that has been converted into uppercase.
		"""#
	examples: [
		{
			title: "Upcase a string"
			input: log: message: #"Here Is A Message"#
			source: #"""
				.message = upcase(.message)
				"""#
			output: log: message: "HERE IS A MESSAGE"
		},
	]
}
