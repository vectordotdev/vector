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
	category: "text"
	description: #"""
		Trims the whitespace from the start and end of the string. [Whitespace](https://en.wikipedia.org/wiki/Unicode_character_property#Whitespace) is any unicode character with the property `"WSpace=yes"`.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				text: #"  A sentence.  "#
			}
			source: #"""
				.trimmed = strip_whitespace(.text)
				"""#
			output: {
				text:  #"  A sentence.  "#
				slice: "A sentence."
			}
		},
		{
			title: "Error"
			input: {
				text: 42
			}
			source: #"""
				.trimmed = strip_whitespace(.text)
				"""#
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
