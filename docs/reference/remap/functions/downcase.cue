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
	internal_failure_reasons: []
	return: ["string"]
	category: "String"
	description: #"""
		Returns a copy of `value` that is entirely lowercase.

		"Lowercase" is defined according to the terms of the Unicode Derived Core Property Lowercase.
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
