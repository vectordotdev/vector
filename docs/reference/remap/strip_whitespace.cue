package metadata

remap: functions: strip_whitespace: {
	arguments: [
		{
			name:        "value"
			description: "The string to trim."
			required:    true
			type: ["string"]
		},
	]
	return: ["string"]
	category: "String"
	description: #"""
		Trims the whitespace from the start and end of the string. [Whitespace](https://en.wikipedia.org/wiki/Unicode_character_property#Whitespace) is any unicode character with the property `"WSpace=yes"`.
		"""#
	examples: [
		{
			title: "Strip whitespace"
			input: log: text: "  A sentence.  "
			source: #"""
				.text = strip_whitespace(.text)
				"""#
			output: log: text: "A sentence."
		},
	]
}
