package metadata

remap: functions: screamingsnakecase: {
	category: "String"
	description: """
		Takes the `value` string, and turns it into SCREAMING_SNAKE case. Optionally, you can
		pass in the existing case of the function, or else we will try to figure out the case automatically.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to convert to SCREAMING_SNAKE case."
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
			title: "SCREAMING_SNAKE a string"
			source: #"""
				screamingsnakecase("input-string")
				"""#
			return: "INPUT_STRING"
		},
		{
			title: "SCREAMING_SNAKE a string"
			source: #"""
				screamingsnakecase("input-string", "kebab-case")
				"""#
			return: "INPUT_STRING"
		},
	]
}
