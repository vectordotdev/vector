package metadata

remap: functions: camelcase: {
	category: "String"
	description: """
		Takes the `value` string, and turns it into camelCase. Optionally, you can
		pass in the existing case of the function, or else an attempt is made to determine the case automatically.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to convert to camelCase."
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
			title: "camelCase a string"
			source: #"""
				camelcase("input-string")
				"""#
			return: "inputString"
		},
		{
			title: "camelCase a string"
			source: #"""
				camelcase("input-string", "kebab-case")
				"""#
			return: "inputString"
		},
	]
}
