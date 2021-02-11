package metadata

remap: functions: starts_with: {
	category: "String"
	description: """
		Determines if the `value` begins with the `substring`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to search."
			required:    true
			type: ["string"]
		},
		{
			name:        "substring"
			description: "The substring `value` must start with."
			required:    true
			type: ["string"]
		},
		{
			name:        "case_sensitive"
			description: "Should the match be case sensitive?"
			required:    false
			type: ["boolean"]
			default: true
		},
	]
	internal_failure_reasons: []
	return: types: ["boolean"]

	examples: [
		{
			title: "String starts with (case sensitive)"
			source: #"""
				starts_with("The Needle In The Haystack", \"The Needle\")
				"""#
			return: true
		},
		{
			title: "String starts with (case insensitive)"
			source: #"""
				starts_with("The Needle In The Haystack", \"the needle\", case_sensitive: false)
				"""#
			return: true
		},
	]
}
