package metadata

remap: functions: ends_with: {
	arguments: [
		{
			name:        "value"
			description: "The string to search."
			required:    true
			type: ["string"]
		},
		{
			name:        "substring"
			description: "The substring `value` must end with."
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
	return: ["boolean"]
	category: "String"
	description: #"""
		Determines if the provided `value` ends with a given `substring`.
		"""#
	examples: [
		{
			title: "String ends with (case sensitive)"
			source: #"""
				ends_with("The Needle In The Haystack", "The Haystack")
				"""#
			return: true
		},
		{
			title: "String ends with (case insensitive)"
			source: #"""
				ends_with("The Needle In The Haystack", "the haystack", case_sensitive: false)
				"""#
			return: true
		},
	]
}
