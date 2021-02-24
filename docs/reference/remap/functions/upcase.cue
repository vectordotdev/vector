package metadata

remap: functions: upcase: {
	description: """
		Upcases the `value`.

		"Upcase" is defined according to the terms of the Unicode Derived Core Property Uppercase.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to convert to uppercase."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]
	category: "String"

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
