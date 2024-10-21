package metadata

remap: functions: kebabcase: {
	category: "String"
	description: """
		Takes the `value` string, and turns it into kebab-case. Optionally, you can
		pass in the existing case of the function, or else we will try to figure out the case automatically.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to convert to kebab-case."
			required:    true
			type: ["string"]
		},
		{
			name:        "original_case"
			description: "Optional hint on the original case type. Must be one of: kebab-case, camelCase, PascalCase, SCREAMING_SNAKE, snake_case"
			required:    false
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "kebab-case a string"
			source: #"""
				kebabcase("InputString")
				"""#
			return: "input-string"
		},
		{
			title: "kebab-case a string"
			source: #"""
				kebabcase("InputString", "PascalCase")
				"""#
			return: "input-string"
		},
	]
}
