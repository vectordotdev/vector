package metadata

remap: functions: contains: {
	category: "String"
	description: """
		Determines if the `value` contains a `substring`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The text to search."
			required:    true
			type: ["string"]
		},
		{
			name:        "substring"
			description: "The substring to search for in `value`."
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
			title: "String contains (case sensitive)"
			source: #"""
				contains("The Needle In The Haystack", "Needle")
				"""#
			return: true
		},
		{
			title: "String contains (case insensitive)"
			source: #"""
				contains("The Needle In The Haystack", "needle", case_sensitive: false)
				"""#
			return: true
		},
	]
}
