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
			description: "The inclusive start position. A zero-based index that can be negative."
			required:    true
			type: ["integer"]
		},
		{
			name:        "end"
			description: "The inclusive end position. A zero-based index that can be negative."
			required:    false
			default:     "String length"
			type: ["integer"]
		},
	]
	internal_failure_reasons: []
	return: ["string"]
	category: "String"
	description: #"""
		Returns a slice of the provided `value` between the `start` and `end` positions specified.

		If the `start` and `end` parameters are negative, they refer to positions counting from the right of the
		string or array. If `end` refers to a position that is greater than the length of the string or array
		a slice up to the end of the string or array is returned.
		"""#
	examples: [
		{
			title: "Slice a string (positve index)"
			source: #"""
				slice("Supercalifragilisticexpialidocious", start: 5, end: 13)
				"""#
			return: "califrag"
		},
		{
			title: "Slice a string (negative index)"
			source: #"""
				slice("Supercalifragilisticexpialidocious", start: 5, end: -14)
				"""#
			return: "califragilistic"
		},
	]
}
