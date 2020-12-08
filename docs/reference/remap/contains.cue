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
	category: "text"
	description: #"""
		Searches a string, `value` to determine if it contains a given `substring`.
		The search can be optionally case insensitive.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				message: #"The Needle In The Haystack"#
			}
			source: #"""
				.contains = contains(.message, "needle", case_sensitive = false)
				"""#
			output: {
				contains: true
			}
		},
		{
			title: "Error"
			input: {
				message: "A string with 42"
			}
			source: ".contains = contains(.message, 42)"
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
