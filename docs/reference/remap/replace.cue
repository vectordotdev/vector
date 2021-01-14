package metadata

remap: functions: replace: {
	arguments: [
		{
			name:        "value"
			description: "The original string."
			required:    true
			type: ["string"]
		},
		{
			name:        "pattern"
			description: "Replace all matches of this pattern. Can be a static string or a regular expression."
			required:    true
			type: ["regex", "string"]
		},
		{
			name:        "with"
			description: "The string that the matches are replaced with."
			required:    true
			type: ["string"]
		},
		{
			name:        "count"
			description: "The maximum number of replacements to perform. -1 means replace all matches."
			required:    false
			default:     -1
			type: ["integer"]

		},
	]
	internal_failure_reasons: []
	return: ["string"]
	category: "String"
	description: #"""
		Replaces any matching patterns in the provided `value` via the provided `pattern`.
		"""#
	examples: [
		{
			title: "Replace literal text"
			input: log: message: #"Apples and Bananas"#
			source: #"""
				.message = replace(.message, "and", "not")
				"""#
			output: log: message: "Apples not Bananas"
		},
		{
			title: "Replace via regular expression"
			input: log: message: #"Apples and Bananas"#
			source: #"""
				.message = replace(.message, /bananas/i, "Pineapples")
				"""#
			output: log: message: "apples and Pineapples"
		},
		{
			title: "Replace first instance"
			input: log: message: #"Bananas and Bananas"#
			source: #"""
				.message = replace(.message, "Bananas", "Pineapples", count: 1)
				"""#
			output: log: message: "Pineapples and Bananas"
		},
	]
}
