package metadata

remap: functions: contains: {
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
	return: ["boolean"]
	category: "String"
	description: #"""
		Searches a string, `value` to determine if it contains a given `substring`.
		The search can be optionally case insensitive.
		"""#
	examples: [
		{
			title: "String contains (case sensitive)"
			input: log: message: #"The Needle In The Haystack"#
			source: #"""
				.contains = contains(.message, "Needle")
				"""#
			output: input & {
				log: contains: true
			}
		},
		{
			title: "String contains (case insensitive)"
			input: log: message: #"The Needle In The Haystack"#
			source: #"""
				.contains = contains(.message, "needle", case_sensitive = false)
				"""#
			output: input & {
				log: contains: true
			}
		},
	]
}
