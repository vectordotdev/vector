package metadata

remap: functions: is_empty: {
	category: "Array"
	description: """
		Check if the array or string is empty or not.
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to check"#
			required:    true
			type: ["array", "string"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `true` if `value` is empty."#,
			#"Returns `false` if `value` is non-empty."#,
		]
	}

	examples: [
		{
			title: "Empty array"
			source: """
				is_empty([])
				"""
			return: true
		},
		{
			title: "Non-empty string"
			source: """
				is_empty("a string")
				"""
			return: false
		},
	]
}
