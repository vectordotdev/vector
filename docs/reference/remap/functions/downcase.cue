package metadata

remap: functions: downcase: {
	category: "String"
	description: """
		Downcases the `value`.

		"Downcase" is defined according to the terms of the Unicode Derived Core Property Lowercase.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to convert to lowercase."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Downcase a string"
			source: #"""
				downcase("Hello, World!")
				"""#
			return: "hello, world!"
		},
	]
}
