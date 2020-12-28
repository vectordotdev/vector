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
			title: "Success"
			input: {
				message: #"Here Is A Message"#
			}
			source: #"""
				.message = upcase(.message)
				"""#
			output: {
				message: "HERE IS A MESSAGE"
			}
		},
		{
			title: "Error"
			input: {
				message: 42
			}
			source: "upcase(.message)"
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
