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
			description: "Replace all matches of this pattern."
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
	return: ["string"]
	category: "String"
	description: #"""
		Replaces any matches of pattern with the provided string. Pattern can be either a fixed string or a regular expression.

		Regular expressions take the form `/regex/flags` where flags are one of the following:

		- *i* perform a case insensitive match.
		- *m* multiline. When enabled `^` and `$` match the beginning and end of multiline strings.
		- *x* ignore whitespace and comments inside the regex.
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
