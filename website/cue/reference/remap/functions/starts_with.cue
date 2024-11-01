package metadata

remap: functions: starts_with: {
	category: "String"
	description: """
		Determines whether `value` begins with `substring`.
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
			description: "The substring that the `value` must start with."
			required:    true
			type: ["string"]
		},
		{
			name:        "case_sensitive"
			description: "Whether the match should be case sensitive."
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
				starts_with("The Needle In The Haystack", "The Needle")
				"""#
			return: true
		},
		{
			title: "String starts with (case insensitive)"
			source: #"""
				starts_with("The Needle In The Haystack", "the needle", case_sensitive: false)
				"""#
			return: true
		},
	]
}
