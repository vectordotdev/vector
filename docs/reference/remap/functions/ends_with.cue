package metadata

remap: functions: ends_with: {
	category: "String"
	description: """
		Determines whether the `value` string ends with the specified `substring`.
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
			description: "The substring with which `value` must end."
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
