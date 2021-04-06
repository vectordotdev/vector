package metadata

remap: functions: is_timestamp: {
	category: "Type"
	description: """
		Check if the type of a `value` is a timestamp or not.
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to check"#
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `true` if `value` is a timestamp."#,
			#"Returns `false` if `value` is anything else."#,
		]
	}

	examples: [
		{
			title: "Valid timestamp"
			source: """
				is_timestamp(t'2021-03-26T16:00:00Z')
				"""
			return: true
		},
		{
			title: "Non-matching type"
			source: """
				is_timestamp("a string")
				"""
			return: false
		},
	]
}
