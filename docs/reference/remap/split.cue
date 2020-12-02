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
	category: "text"
	description: #"""
		Splits the given string whenever a given pattern is matched. If `limit` is specified, after `limit` has been reached
		the remainder of the string is returned unsplit.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				text: "apples and pears and bananas"
			}
			source: #"""
				.split = split(.text, " and ")
				"""#
			output: {
				text: "apples and pears and bananas"
				split: ["apples", "pears", "bananas"]
			}
		},
		{
			title: "Split Limit"
			input: {
				text: "apples and pears and bananas"
			}
			source: #"""
				.split = split(.text, " and ", 1)
				"""#
			output: {
				text: "apples and pears and bananas"
				split: ["apples", "pears and bananas"]
			}
		},
		{
			title: "Error"
			input: {
				text: 42
			}
			source: #"""
				.split = split(.text, " and ")
				"""#
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
