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
	category: "text"
	description: #"""
		Returns a slice of the provided string or array between the `start` and `end` positions specified.

		If the `start` and `end` parameters are negative, they refer to positions counting from the right of the
		string or array. If `end` refers to a position that is greater than the length of the string or array
		a slice up to the end of the string or array is returned.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				text: #"Supercalifragilisticexpialidocious"#
			}
			source: #"""
				.slice = slice(.text, start=5, end=13)
				"""#
			output: {
				text:  #"Supercalifragilisticexpialidocious"#
				slice: "califrag"
			}
		},
		{
			title: "From End"
			input: {
				text: #"Supercalifragilisticexpialidocious"#
			}
			source: #"""
				.slice = slice(.text, start=5, end=-14)
				"""#
			output: {
				text:  #"Supercalifragilisticexpialidocious"#
				slice: "califragilistic"
			}
		},
		{
			title: "Error"
			input: {
				text: #"Super"#
			}
			source: #"""
				.slice = slice(.text, start=10)
				"""#
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
