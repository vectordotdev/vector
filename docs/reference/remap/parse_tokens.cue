package metadata

remap: functions: parse_tokens: {
	arguments: [
		{
			name:        "value"
			description: "The string to tokenize."
			required:    true
			type: ["string"]
		},
	]
	return: ["array"]
	category: "Parse"
	description: #"""
		Splits the string up into an array of tokens. A token is considered to be:
		- A word surrounded by whitespace.
		- Text delimited by double quotes: `".."`. Quotes can be included in the token if they are escaped by a backslash (`\`).
		- Text delimited by square brackets: `[..]`. Closing square brackets can be included in the token if they are escaped by a backslash (`\`).
		"""#
	examples: [
		{
			title: "Success"
			input: {
				text: #"""
					A sentence "with \"a\" sentence inside" and [some brackets]
					"""#
			}
			source: #"""
				.tokens = parse_tokens(.text)
				"""#
			output: {
				text: #"""
					A sentence "with \"a\" sentence inside" and [some brackets]
					"""#
				slice: ["A", "sentence", #"with \"a\" sentence inside"#, "and", "some brackets"]
			}
		},
	]
}
