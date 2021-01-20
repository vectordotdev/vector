package metadata

remap: functions: match: {
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
	return: ["boolean"]
	category: "String"
	description: """
		Returns `true` if the provided `value` matches the provided `pattern`.
		"""
	examples: [
		{
			title: "Regex match on a string"
			source: """
				match("I'm a little teapot", /teapot/)
				"""
			return: true
		},
	]
}
