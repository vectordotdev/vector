package metadata

remap: functions: truncate: {
	category: "String"
	description: """
		Truncates the `value` string up to the `limit` number of characters.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to truncate."
			required:    true
			type: ["string"]
		},
		{
			name:        "limit"
			description: "The number of characters to truncate the string after."
			required:    true
			type: ["integer", "float"]
		},
		{
			name: "ellipsis"
			description: """
				An ellipsis (`...`) is appended if this is set to `true` _and_ the `value` string ends up being
				truncated because it's exceeded the `limit`.
				"""
			required: true
			type: ["boolean"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["string"]
		rules: [
			"The string is returned unchanged its length is less than `limit`.",
			"If `ellipsis` is `true`, then an ellipsis (`...`) is appended to the string (beyond the specified `limit`).",
		]
	}

	examples: [
		{
			title: "Truncate a string"
			source: #"""
				truncate("A rather long sentence.", limit: 11, ellipsis: true)
				"""#
			return: "A rather lo..."
		},
	]
}
