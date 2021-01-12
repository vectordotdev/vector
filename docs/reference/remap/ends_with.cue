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
	return: ["boolean"]
	category: "String"
	description: #"""
		Determines if a given string ends with a given `substring`.
		The search can be optionally case insensitive.
		"""#
	examples: [
		{
			title: "String ends with"
			input: log: message: #"The Needle In The Haystack"#
			source: #"""
				.contains = ends_with(.message, "the haystack", case_sensitive: false)
				"""#
			output: input & {
				log: contains: true
			}
		},
	]
}
