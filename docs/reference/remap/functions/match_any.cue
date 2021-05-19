package metadata

remap: functions: match_any: {
	category: "String"
	description: """
		Determines whether the `value` matches any the given `patterns`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to match."
			required:    true
			type: ["string"]
		},
		{
			name:        "patterns"
			description: "The array of regular expression patterns to match against."
			required:    true
			type: ["array"]
		},
	]
	internal_failure_reasons: []
	return: types: ["boolean"]

	examples: [
		{
			title: "Regex match on a string"
			source: """
				match_any("I'm a little teapot", [r'frying pan', r'teapot'])
				"""
			return: true
		},
	]
}
