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
	category: "text"
	description: #"""
		Determines if a given string ends with a given `substring`.
		The search can be optionally case insensitive.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				message: #"The Needle In The Haystack"#
			}
			source: #"""
				.contains = ends_with(.message, "the haystack", case_sensitive = false)
				"""#
			output: {
				message:  #"The Needle In The Haystack"#
				contains: true
			}
		},
		{
			title: "Error"
			input: {
				message: "A string with 42"
			}
			source: ".contains = ends_with(.message, 42)"
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
