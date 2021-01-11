package metadata

remap: functions: downcase: {
	arguments: [
		{
			name:        "value"
			description: "The string to convert to lowercase."
			required:    true
			type: ["string"]
		},
	]
	return: ["string"]
	category: "String"
	description: #"""
		Returns a copy of `string` that has been converted into lowercase.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				message: #"Here Is A Message"#
			}
			source: #"""
				.message = downcase(.message)
				"""#
			output: {
				message: "here is a message"
			}
		},
		{
			title: "Error"
			input: {
				message: 42
			}
			source: "downcase(.message)"
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
