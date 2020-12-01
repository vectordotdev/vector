package metadata

remap: functions: starts_with: {
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
	return: ["boolean"]
	category: "text"
	description: #"""
		Determines if a given string begins with a given `substring`.
		The search can be optionally case insensitive.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				message: #"The Needle In The Haystack"#
			}
			source: #"""
				.starts = starts_with(.message, \"the needle\", case_sensitive = false)
				"""#
			output: {
				message: #"The Needle In The Haystack"#
				starts:  true
			}
		},
		{
			title: "Invalid \"substring\" argument type"
			input: {
				message: "A string with 42"
			}
			source: ".starts = starts_with(.message, 42)"
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
