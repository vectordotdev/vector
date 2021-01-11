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
			title: "Downcase a string"
			input: log: message: #"Here Is A Message"#
			source: #"""
				.message = downcase(.message)
				"""#
			output: log: message: "here is a message"
		},
	]
}
