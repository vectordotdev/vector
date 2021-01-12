package metadata

remap: functions: split: {
	arguments: [
		{
			name:        "value"
			description: "The string to split."
			required:    true
			type: ["string"]
		},
		{
			name:        "pattern"
			description: "The string is split whenever this pattern is matched."
			required:    true
			type: ["string", "regex"]
		},
		{
			name:        "limit"
			description: "The maximum number of substrings to return."
			required:    false
			type: ["integer"]
		},
	]
	return: ["string"]
	category: "String"
	description: #"""
		Splits the given string whenever a given pattern is matched. If `limit` is specified, after `limit` has been reached
		the remainder of the string is returned unsplit.
		"""#
	examples: [
		{
			title: "Split a string (no limit)"
			input: log: text: "apples and pears and bananas"
			source: #"""
				.text = split(.text, " and ")
				"""#
			output: log: text: ["apples", "pears", "bananas"]
		},
		{
			title: "Split a string (with a limit)"
			input: log: text: "apples and pears and bananas"
			source: #"""
				.text = split(.text, " and ", 1)
				"""#
			output: log: text: ["apples", "pears and bananas"]
		},
	]
}
