package metadata

remap: functions: match: {
	category: "String"
	description: """
		Determines whether the `value` matches the `pattern`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to match."
			required:    true
			type: ["string"]
		},
		{
			name:        "pattern"
			description: "The regular expression pattern to match against."
			required:    true
			type: ["regex"]
		},
	]
	internal_failure_reasons: []
	return: types: ["boolean"]

	examples: [
		{
			title: "Regex match on a string"
			source: """
				match("I'm a little teapot", r'teapot')
				"""
			return: true
		},
		{
			title: "String does not match the regular expression"
			source: """
				match("I'm a little teapot", r'.*balloon')
				"""
			return: false
		},
	]
}
