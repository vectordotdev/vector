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
	category: "text"
	description: #"""
		Replaces any matches of pattern with the provided string. Pattern can be either a fixed string or a regular expression.

		Regular expressions take the form `/regex/flags` where flags are one of the following:

		- *i* perform a case insensitive match.
		- *m* multiline. When enabled `^` and `$` match the beginning and end of multiline strings.
		- *x* ignore whitespace and comments inside the regex.
		"""#
	examples: [
		{
			title: "Text match"
			input: {
				text: #"Apples and Bananas"#
			}
			source: #"""
				.replaced = replace(.text, "and", "not")
				"""#
			output: {
				text:     #"Apples and Bananas"#
				replaced: "Apples not Bananas"
			}
		},
		{
			title: "Regular expression match"
			input: {
				text: #"Apples and Bananas"#
			}
			source: #"""
				.replaced = replace(.text, /bananas/i, "Pineapples")
				"""#
			output: {
				text:     #"Apples and Bananas"#
				replaced: "apples and Pineapples"
			}
		},
		{
			title: "Replace first instance"
			input: {
				text: #"Bananas and Bananas"#
			}
			source: #"""
				.replaced = replace(.text, "Bananas", "Pineapples", count = 1)
				"""#
			output: {
				text:     #"Apples and Bananas"#
				replaced: "Pineapples and Bananas"
			}
		},
		{
			title: "Error"
			input: {
				text: 42
			}
			source: #"""
				.replaced = replace(.text, "42", "43")
				"""#
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
