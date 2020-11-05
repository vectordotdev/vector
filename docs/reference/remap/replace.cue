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
	]
	return: ["string"]
	category: "text"
	description: #"""
			Replaces any matches of pattern with the provided string. Pattern can be either a fixed string or a regular expression.

			Regular expressions take the form `/<regex>/<flags> where flags are one of the following:

			- *i* perform a case insensitive match.
			- *g* global. If specified all occurrences of the pattern are replaced. 
			- *m* multiline. When enabled `^` and `$` match the beginning and end of multiline strings.
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
				replaced: "Apples not Bananas"
			}
		},
		{
			title: "Regular expression match"
			input: {
				text: #"Apples and Bananas"#
			}
			source: #"""
				.replaced = replace(.text, /bananas/i, "Pineapples)
				"""#
			output: {
				replaced: "apples and Pineapples"
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
