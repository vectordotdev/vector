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
	internal_failure_reasons: []
	return: ["string"]
	category: "String"
	description: #"""
		Returns a copy of `value` that is entirely uppercase.

		"Uppercase" is defined according to the terms of the Unicode Derived Core Property Uppercase.
		"""#
	examples: [
		{
			title: "Upcase a string"
			source: #"""
				upcase("Hello, World!")
				"""#
			output: log: message: "HELLO, WORLD!"
		},
	]
}
