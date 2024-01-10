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
				This argument is deprecated. An ellipsis (`...`) is appended if the parameter is set to `true` _and_ the `value` string
				is truncated because it exceeded the `limit`.
				"""
			required: false
			type: ["boolean"]
		},
		{
			name: "suffix"
			description: """
				A custom suffix (`...`) is appended to truncated strings.
				If `ellipsis` is set to `true`, this parameter is ignored for backwards compatibility.
				"""
			required: false
			type: ["string"]
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
				truncate("A rather long sentence.", limit: 11, suffix: "...")
				"""#
			return: "A rather lo..."
		},
		{
			title: "Truncate a string"
			source: #"""
				truncate("A rather long sentence.", limit: 11, suffix: "[TRUNCATED]")
				"""#
			return: "A rather lo[TRUNCATED]"
		},
	]
}
