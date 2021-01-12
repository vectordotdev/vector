package metadata

remap: functions: slice: {
	arguments: [
		{
			name:        "value"
			description: "The string or array to slice."
			required:    true
			type: ["array", "string"]
		},
		{
			name:        "start"
			description: "The start position."
			required:    true
			type: ["integer"]
		},
		{
			name:        "end"
			description: "The end position."
			required:    false
			default:     "String length"
			type: ["integer"]
		},
	]
	return: ["string"]
	category: "String"
	description: #"""
		Returns a slice of the provided string or array between the `start` and `end` positions specified.

		If the `start` and `end` parameters are negative, they refer to positions counting from the right of the
		string or array. If `end` refers to a position that is greater than the length of the string or array
		a slice up to the end of the string or array is returned.
		"""#
	examples: [
		{
			title: "Slice a string (positve index)"
			input: log: text: #"Supercalifragilisticexpialidocious"#
			source: #"""
				.text = slice(.text, start: 5, end: 13)
				"""#
			output: log: text: "califrag"
		},
		{
			title: "Slice a string (negative index)"
			input: log: text: #"Supercalifragilisticexpialidocious"#
			source: #"""
				.text = slice(.text, start: 5, end: -14)
				"""#
			output: log: text: "califragilistic"
		},
	]
}
