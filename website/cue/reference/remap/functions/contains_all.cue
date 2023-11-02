package metadata

remap: functions: contains_all: {
	category: "String"
	description: """
		Determines whether the `value` string contains all the specified `substrings`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The text to search."
			required:    true
			type: ["string"]
		},
		{
			name:        "substrings"
			description: "An array of substrings to search for in `value`."
			required:    true
			type: ["array"]
		},
		{
			name:        "case_sensitive"
			description: "Whether the match should be case sensitive."
			required:    false
			type: ["boolean"]
		},
	]
	internal_failure_reasons: []
	return: types: ["boolean"]

	examples: [
		{
			title: "String contains all"
			source: #"""
				contains_all("The Needle In The Haystack", ["Needle", "Haystack"])
				"""#
			return: true
		},
		{
			title: "String contains all (case sensitive)"
			source: #"""
				contains_all("the NEEDLE in the haystack", ["needle", "haystack"])
				"""#
			return: false
		},
	]

}
