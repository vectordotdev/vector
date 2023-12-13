package metadata

remap: functions: split: {
	category: "String"
	description: """
		Splits the `value` string using `pattern`.
		"""

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
	internal_failure_reasons: []
	return: {
		types: ["array"]
		rules: [
			"If `limit` is specified, the remainder of the string is returned unsplit after `limit` has been reached.",
		]
	}

	examples: [
		{
			title: "Split a string (no limit)"
			source: #"""
				split("apples and pears and bananas", " and ")
				"""#
			return: ["apples", "pears", "bananas"]
		},
		{
			title: "Split a string (with a limit)"
			source: #"""
				split("apples and pears and bananas", " and ", limit: 2)
				"""#
			return: ["apples", "pears and bananas"]
		},
	]
}
